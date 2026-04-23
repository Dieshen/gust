//! Branch-level coverage for `codegen_wasm.rs`.
//!
//! Complements `target_backends.rs` (which covers the happy path) by
//! exercising: struct + enum type decls, primitive and composite type
//! mappings, sync vs timeout transitions, generic machines, and
//! `Default::default()`.

use gust_lang::{WasmCodegen, parse_program_with_errors};

fn r#gen(source: &str) -> String {
    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    WasmCodegen::new().generate(&program)
}

#[test]
fn struct_type_decl_emits_wasm_bindgen_struct() {
    let src = r#"
type Point { x: i64, y: i64 }

machine M {
    state S
}
"#;
    let out = r#gen(src);
    assert!(out.contains("pub struct Point"));
    assert!(out.contains("pub x: i64"));
    assert!(out.contains("pub y: i64"));
}

#[test]
fn enum_type_decl_emits_repr_u32_enum_with_indices() {
    let src = r#"
enum Color { Red, Green, Blue }

machine M {
    state S
}
"#;
    let out = r#gen(src);
    assert!(out.contains("pub enum Color"));
    assert!(out.contains("Red = 0"));
    assert!(out.contains("Green = 1"));
    assert!(out.contains("Blue = 2"));
    assert!(out.contains("#[repr(u32)]"));
}

#[test]
fn primitive_type_mappings_preserved() {
    // wasm_type maps i32/u32/u64/f32/f64/bool/String directly; unknown → JsValue.
    let src = r#"
type Bag { a: i32, b: u32, c: u64, d: f32, e: f64, f: bool, g: String, h: Custom }

machine M {
    state S
}
"#;
    let out = r#gen(src);
    assert!(out.contains("pub a: i32"));
    assert!(out.contains("pub b: u32"));
    assert!(out.contains("pub c: u64"));
    assert!(out.contains("pub d: f32"));
    assert!(out.contains("pub e: f64"));
    assert!(out.contains("pub f: bool"));
    assert!(out.contains("pub g: String"));
    // Unknown custom type should fall back to JsValue.
    assert!(out.contains("pub h: JsValue"));
}

#[test]
fn generic_and_tuple_types_become_jsvalue() {
    let src = r#"
type Box { items: Vec<i64>, pair: (i64, String) }

machine M {
    state S
}
"#;
    let out = r#gen(src);
    // Both Generic and Tuple map to JsValue per codegen policy.
    assert!(out.contains("pub items: JsValue"));
    assert!(out.contains("pub pair: JsValue"));
}

#[test]
fn sync_transition_emits_result_jsvalue() {
    let src = r#"
machine Door {
    state Closed
    state Open
    transition open: Closed -> Open
    on open() { goto Open(); }
}
"#;
    let out = r#gen(src);
    assert!(out.contains("pub fn open(&mut self) -> Result<(), JsValue>"));
    assert!(out.contains("return Err(JsValue::from_str(\"invalid transition\"))"));
    assert!(out.contains("self.state = DoorState::Open"));
}

#[test]
fn timeout_transition_emits_promise_async_branch() {
    let src = r#"
machine Net {
    state Idle
    state Active
    transition connect: Idle -> Active timeout 5s
    on connect() { goto Active(); }
}
"#;
    let out = r#gen(src);
    assert!(out.contains("connectAsync"));
    assert!(out.contains("pub fn connect_async(&mut self) -> Promise"));
    assert!(out.contains("Promise::reject"));
    assert!(out.contains("future_to_promise"));
}

#[test]
fn generic_machine_emits_generic_bounds() {
    let src = r#"
machine Box<T> {
    state Empty
    state Full
    transition fill: Empty -> Full
    on fill() { goto Full(); }
}
"#;
    let out = r#gen(src);
    assert!(out.contains("T: Into<JsValue> + Clone"));
    assert!(out.contains("pub struct Box<T"));
    assert!(out.contains("pub enum BoxState<T"));
}

#[test]
fn constructor_uses_first_state() {
    let src = r#"
machine Light {
    state Off
    state On
}
"#;
    let out = r#gen(src);
    assert!(out.contains("#[wasm_bindgen(constructor)]"));
    assert!(out.contains("Self { state: LightState::Off }"));
}

#[test]
fn state_method_returns_u32_discriminant() {
    let src = r#"
machine X { state A }
"#;
    let out = r#gen(src);
    assert!(out.contains("pub fn state(&self) -> u32"));
    assert!(out.contains("self.state as u32"));
}

#[test]
fn default_impl_equivalent_to_new() {
    // Route through the Default trait explicitly. We can't write
    // `WasmCodegen::default()` directly because clippy 1.95's
    // `default_constructed_unit_structs` lint rejects it for unit
    // structs even when a Default impl exists.
    let a: WasmCodegen = Default::default();
    let b = WasmCodegen::new();
    let src = "machine M { state S }";
    let program = parse_program_with_errors(src, "t.gu").expect("parses");
    // Both should produce byte-identical output.
    assert_eq!(a.generate(&program), b.generate(&program));
}

#[test]
fn empty_program_still_emits_prelude_and_adapter_trait() {
    let out = r#gen("");
    assert!(out.contains("use wasm_bindgen::prelude::*;"));
    assert!(out.contains("pub trait GustWasmEffectAdapter"));
    assert!(out.contains("call_effect"));
}
