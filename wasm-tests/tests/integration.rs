//! SPDX-License-Identifier: MIT
//!
//! Copyright (c) 2026 Kevin Thomas
//!
//! # Integration Tests for Wasm UART Echo Component
//!
//! Validates that the compiled Wasm component loads correctly through the
//! Component Model, implements the expected WIT interface
//! (`embedded:platform/uart`), exports the `run` function, and echoes
//! characters properly including backspace handling.

use wasmtime::component::{Component, HasSelf};
use wasmtime::{Config, Engine, Store};

wasmtime::component::bindgen!({
    world: "uart-echo",
    path: "../wit",
});

/// Compiled Wasm UART echo component embedded at build time.
const WASM_BINARY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/uart_echo.wasm"));

/// Represents a single host function call recorded during Wasm execution.
#[derive(Debug, PartialEq)]
enum HostCall {
    /// The `uart.read-byte` WIT function was called, returning the given byte.
    ReadByte(u8),
    /// The `uart.write-byte` WIT function was called with the given byte.
    WriteByte(u8),
}

/// Host state that records all function calls and feeds input bytes.
struct TestHostState {
    /// Ordered log of every host function call.
    calls: Vec<HostCall>,
    /// Queue of bytes to feed to `uart.read-byte` calls.
    input: Vec<u8>,
    /// Current position in the input queue.
    input_pos: usize,
}

impl embedded::platform::uart::Host for TestHostState {
    /// Returns the next byte from the input queue, or 0 if exhausted.
    ///
    /// # Returns
    ///
    /// The next input byte, or 0 if the input queue is exhausted.
    fn read_byte(&mut self) -> u8 {
        let byte = if self.input_pos < self.input.len() {
            let b = self.input[self.input_pos];
            self.input_pos += 1;
            b
        } else {
            0
        };
        self.calls.push(HostCall::ReadByte(byte));
        byte
    }

    /// Records the written byte in the call log.
    ///
    /// # Arguments
    ///
    /// * `byte` - The byte written by the Wasm guest.
    fn write_byte(&mut self, byte: u8) {
        self.calls.push(HostCall::WriteByte(byte));
    }
}

/// Creates a wasmtime engine with fuel metering enabled.
///
/// # Returns
///
/// A wasmtime `Engine` with fuel consumption enabled.
///
/// # Panics
///
/// Panics if engine creation fails.
fn create_fuel_engine() -> Engine {
    let mut config = Config::default();
    config.consume_fuel(true);
    Engine::new(&config).expect("create fuel engine")
}

/// Creates a default wasmtime engine without fuel metering.
///
/// # Returns
///
/// A wasmtime `Engine` with default configuration.
fn create_default_engine() -> Engine {
    Engine::default()
}

/// Compiles the embedded Wasm binary into a wasmtime component.
///
/// # Arguments
///
/// * `engine` - The wasmtime engine to compile with.
///
/// # Returns
///
/// The compiled Wasm `Component`.
///
/// # Panics
///
/// Panics if the Wasm binary is invalid.
fn compile_component(engine: &Engine) -> Component {
    Component::new(engine, WASM_BINARY).expect("valid Wasm component")
}

/// Builds a fully configured test linker with all WIT interfaces registered.
///
/// # Arguments
///
/// * `engine` - The wasmtime engine to associate the linker with.
///
/// # Returns
///
/// A component `Linker` with `uart::Host` registered.
///
/// # Panics
///
/// Panics if WIT interface registration fails.
fn build_test_linker(engine: &Engine) -> wasmtime::component::Linker<TestHostState> {
    let mut linker = wasmtime::component::Linker::new(engine);
    UartEcho::add_to_linker::<TestHostState, HasSelf<TestHostState>>(
        &mut linker,
        |state: &mut TestHostState| state,
    )
    .expect("register WIT interfaces");
    linker
}

/// Creates a store with input data and fuel budget.
///
/// # Arguments
///
/// * `engine` - The wasmtime engine to create the store for.
/// * `input` - Input bytes to feed to `uart.read-byte`.
/// * `fuel` - The amount of fuel to allocate for execution.
///
/// # Returns
///
/// A `Store` containing a `TestHostState` with input and fuel budget set.
///
/// # Panics
///
/// Panics if fuel allocation fails.
fn create_fueled_store(engine: &Engine, input: Vec<u8>, fuel: u64) -> Store<TestHostState> {
    let mut store = Store::new(
        engine,
        TestHostState {
            calls: Vec::new(),
            input,
            input_pos: 0,
        },
    );
    store.set_fuel(fuel).expect("set fuel");
    store
}

/// Runs the Wasm `run` function until fuel is exhausted.
///
/// # Arguments
///
/// * `store` - The wasmtime store with fuel and host state.
/// * `linker` - The component linker with WIT interfaces registered.
/// * `component` - The compiled Wasm component.
///
/// # Panics
///
/// Panics if component instantiation fails.
fn run_until_out_of_fuel(
    store: &mut Store<TestHostState>,
    linker: &wasmtime::component::Linker<TestHostState>,
    component: &Component,
) {
    let uart_echo =
        UartEcho::instantiate(&mut *store, component, linker).expect("instantiate component");
    let _ = uart_echo.call_run(&mut *store);
}

/// Extracts only the `WriteByte` calls from the call log.
///
/// # Arguments
///
/// * `calls` - Slice of recorded host function calls.
///
/// # Returns
///
/// A `Vec<u8>` containing only the bytes from `WriteByte` calls.
fn get_writes(calls: &[HostCall]) -> Vec<u8> {
    calls
        .iter()
        .filter_map(|c| match c {
            HostCall::WriteByte(b) => Some(*b),
            _ => None,
        })
        .collect()
}

/// Verifies that the Wasm component binary loads without error.
///
/// # Panics
///
/// Panics if the Wasm component binary fails to compile.
#[test]
fn test_wasm_component_loads() {
    let engine = create_default_engine();
    let _component = compile_component(&engine);
}

/// Verifies that the component instantiates and exports the `run` function.
///
/// # Panics
///
/// Panics if the component fails to instantiate.
#[test]
fn test_wasm_exports_run_function() {
    let engine = create_default_engine();
    let component = compile_component(&engine);
    let linker = build_test_linker(&engine);
    let mut store = Store::new(
        &engine,
        TestHostState {
            calls: Vec::new(),
            input: Vec::new(),
            input_pos: 0,
        },
    );
    let uart_echo = UartEcho::instantiate(&mut store, &component, &linker);
    assert!(
        uart_echo.is_ok(),
        "component must instantiate with run export"
    );
}

/// Verifies that the component imports the `uart` interface.
///
/// # Panics
///
/// Panics if a required interface import is missing.
#[test]
fn test_wasm_imports_match_expected() {
    let engine = create_default_engine();
    let component = compile_component(&engine);
    let ty = component.component_type();
    let import_names: Vec<_> = ty
        .imports(&engine)
        .map(|(name, _)| name.to_string())
        .collect();
    assert!(
        import_names.iter().any(|n| n.contains("uart")),
        "missing uart interface"
    );
}

/// Verifies that all imports originate from the `embedded:platform` package.
///
/// # Panics
///
/// Panics if any import is not from the `embedded:platform` package.
#[test]
fn test_all_imports_from_embedded_platform() {
    let engine = create_default_engine();
    let component = compile_component(&engine);
    let ty = component.component_type();
    for (name, _) in ty.imports(&engine) {
        assert!(
            name.starts_with("embedded:platform/"),
            "import '{name}' must be from embedded:platform"
        );
    }
}

/// Verifies that normal characters are echoed back unchanged.
///
/// # Panics
///
/// Panics if the echoed output does not match the input characters.
#[test]
fn test_echo_normal_characters() {
    let engine = create_fuel_engine();
    let component = compile_component(&engine);
    let linker = build_test_linker(&engine);
    let input = vec![b'H', b'i'];
    let mut store = create_fueled_store(&engine, input, 500_000);
    run_until_out_of_fuel(&mut store, &linker, &component);
    let writes = get_writes(&store.data().calls);
    assert!(writes.len() >= 2, "need at least 2 writes");
    assert_eq!(writes[0], b'H');
    assert_eq!(writes[1], b'i');
}

/// Verifies that backspace (0x08) produces the erase sequence.
///
/// # Panics
///
/// Panics if the backspace sequence is incorrect.
#[test]
fn test_echo_backspace_handling() {
    let engine = create_fuel_engine();
    let component = compile_component(&engine);
    let linker = build_test_linker(&engine);
    let input = vec![0x08];
    let mut store = create_fueled_store(&engine, input, 500_000);
    run_until_out_of_fuel(&mut store, &linker, &component);
    let writes = get_writes(&store.data().calls);
    assert!(writes.len() >= 3, "backspace must produce 3 writes");
    assert_eq!(writes[0], 0x08, "first byte must be BS");
    assert_eq!(writes[1], b' ', "second byte must be space");
    assert_eq!(writes[2], 0x08, "third byte must be BS");
}

/// Verifies that DEL (0x7F) produces the same erase sequence as backspace.
///
/// # Panics
///
/// Panics if the DEL erase sequence is incorrect.
#[test]
fn test_echo_delete_handling() {
    let engine = create_fuel_engine();
    let component = compile_component(&engine);
    let linker = build_test_linker(&engine);
    let input = vec![0x7F];
    let mut store = create_fueled_store(&engine, input, 500_000);
    run_until_out_of_fuel(&mut store, &linker, &component);
    let writes = get_writes(&store.data().calls);
    assert!(writes.len() >= 3, "DEL must produce 3 writes");
    assert_eq!(writes[0], 0x08, "first byte must be BS");
    assert_eq!(writes[1], b' ', "second byte must be space");
    assert_eq!(writes[2], 0x08, "third byte must be BS");
}

/// Verifies that carriage return produces CR+LF.
///
/// # Panics
///
/// Panics if the newline sequence is not CR followed by LF.
#[test]
fn test_echo_carriage_return_handling() {
    let engine = create_fuel_engine();
    let component = compile_component(&engine);
    let linker = build_test_linker(&engine);
    let input = vec![b'\r'];
    let mut store = create_fueled_store(&engine, input, 500_000);
    run_until_out_of_fuel(&mut store, &linker, &component);
    let writes = get_writes(&store.data().calls);
    assert!(writes.len() >= 2, "CR must produce CR+LF");
    assert_eq!(writes[0], b'\r');
    assert_eq!(writes[1], b'\n');
}

/// Verifies the full echo sequence: normal char, DEL erase, char, CR+LF.
///
/// # Panics
///
/// Panics if the full output sequence does not match expectations.
#[test]
fn test_echo_full_sequence() {
    let engine = create_fuel_engine();
    let component = compile_component(&engine);
    let linker = build_test_linker(&engine);
    let input = vec![b'A', 0x7F, b'B', b'\r'];
    let mut store = create_fueled_store(&engine, input, 1_000_000);
    run_until_out_of_fuel(&mut store, &linker, &component);
    let writes = get_writes(&store.data().calls);
    assert!(writes.len() >= 7, "need full sequence output");
    assert_eq!(writes[0], b'A');
    assert_eq!(writes[1], 0x08);
    assert_eq!(writes[2], b' ');
    assert_eq!(writes[3], 0x08);
    assert_eq!(writes[4], b'B');
    assert_eq!(writes[5], b'\r');
    assert_eq!(writes[6], b'\n');
}
