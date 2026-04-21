//! Branch-level coverage for `codegen_nostd.rs`.
//!
//! Complements `target_backends.rs` by exercising: state with and
//! without fields, constructors for both shapes, transitions from
//! field-bearing states (`{ .. }` pattern), generic machines, and
//! every arm of the `nostd_type` mapping (Vec/Option/Result/other
//! generic, tuple, primitives, unknown Simple, Unit).

use gust_lang::{parse_program_with_errors, NoStdCodegen};

fn gen(source: &str) -> String {
    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    NoStdCodegen::new().generate(&program)
}

#[test]
fn empty_state_generates_empty_constructor() {
    let src = r#"
machine Light {
    state Off
    state On
}
"#;
    let out = gen(src);
    assert!(out.contains("pub fn new() -> Self"));
    assert!(out.contains("Self { state: LightState::Off }"));
}

#[test]
fn field_bearing_first_state_generates_parameterised_constructor() {
    let src = r#"
machine Dev {
    state Idle(name: String, count: i64)
    state Active
}
"#;
    let out = gen(src);
    assert!(out.contains("pub fn new(name: HString<64>, count: i64) -> Self"));
    assert!(out.contains("DevState::Idle { name, count }"));
}

#[test]
fn transitions_from_field_bearing_states_use_struct_pattern() {
    let src = r#"
machine Dev {
    state Idle(name: String)
    state Active
    transition go: Idle -> Active
    on go() { goto Active(); }
}
"#;
    let out = gen(src);
    assert!(out.contains("DevState::Idle { .. }"));
    assert!(out.contains("self.state = DevState::Active"));
}

#[test]
fn transitions_from_empty_states_use_bare_pattern() {
    let src = r#"
machine Door {
    state Closed
    state Open
    transition open: Closed -> Open
    on open() { goto Open(); }
}
"#;
    let out = gen(src);
    assert!(out.contains("DoorState::Closed =>"));
    // Should NOT contain the `{ .. }` pattern form.
    assert!(!out.contains("DoorState::Closed {"));
}

#[test]
fn vec_type_maps_to_hvec() {
    let src = r#"
machine Buf {
    state Holding(items: Vec<i64>)
}
"#;
    let out = gen(src);
    assert!(out.contains("items: HVec<i64, 16>"));
}

#[test]
fn option_type_preserves_option_wrapper() {
    let src = r#"
machine Box {
    state Filled(value: Option<i64>)
}
"#;
    let out = gen(src);
    assert!(out.contains("value: Option<i64>"));
}

#[test]
fn result_type_preserves_both_arms() {
    let src = r#"
machine Outcome {
    state Done(value: Result<i64, String>)
}
"#;
    let out = gen(src);
    // Inner String maps to HString<64> in the nostd mapping.
    assert!(out.contains("value: Result<i64, HString<64>>"));
}

#[test]
fn unknown_generic_passes_through_with_mapped_args() {
    let src = r#"
machine Holder {
    state Holding(val: Custom<i64, String>)
}
"#;
    let out = gen(src);
    assert!(out.contains("val: Custom<i64, HString<64>>"));
}

#[test]
fn tuple_type_flattened() {
    let src = r#"
machine Pair {
    state Holding(p: (i64, String, bool))
}
"#;
    let out = gen(src);
    assert!(out.contains("p: (i64, HString<64>, bool)"));
}

#[test]
fn unknown_simple_type_passes_through() {
    let src = r#"
machine Mystery {
    state Holding(val: MyCustomType)
}
"#;
    let out = gen(src);
    assert!(out.contains("val: MyCustomType"));
}

#[test]
fn all_primitive_types_preserved() {
    let src = r#"
machine Prim {
    state Holding(a: i32, b: u32, c: u64, d: f32, e: f64, f: bool)
}
"#;
    let out = gen(src);
    for pat in ["a: i32", "b: u32", "c: u64", "d: f32", "e: f64", "f: bool"] {
        assert!(out.contains(pat), "missing {pat} in output:\n{out}");
    }
}

#[test]
fn generic_machine_emits_clone_bound() {
    let src = r#"
machine Container<T> {
    state Empty
    state Full
    transition fill: Empty -> Full
    on fill() { goto Full(); }
}
"#;
    let out = gen(src);
    assert!(out.contains("T: Clone"));
    assert!(out.contains("impl<T: Clone> Container<T>"));
    assert!(out.contains("pub enum ContainerState<T: Clone>"));
}

#[test]
fn default_impl_equivalent_to_new() {
    let a = NoStdCodegen::default();
    let b = NoStdCodegen::new();
    let src = "machine M { state S }";
    let program = parse_program_with_errors(src, "t.gu").expect("parses");
    assert_eq!(a.generate(&program), b.generate(&program));
}

#[test]
fn prelude_always_emitted() {
    let out = gen("machine M { state S }");
    assert!(out.contains("#![no_std]"));
    assert!(out.contains("extern crate alloc;"));
    assert!(out.contains("use heapless::{String as HString, Vec as HVec};"));
}

#[test]
fn transition_returns_err_on_invalid_state() {
    let src = r#"
machine Door {
    state Closed
    state Open
    transition open: Closed -> Open
    on open() { goto Open(); }
}
"#;
    let out = gen(src);
    assert!(out.contains("_ => Err(\"invalid transition\")"));
}
