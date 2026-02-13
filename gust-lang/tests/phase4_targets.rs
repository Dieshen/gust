use gust_lang::{parse_program_with_errors, CffiCodegen, NoStdCodegen, WasmCodegen};

#[test]
fn wasm_codegen_emits_wasm_bindgen_prelude() {
    let source = r#"
machine Counter {
    state Ready
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    let out = WasmCodegen::new().generate(&program);
    assert!(out.contains("use wasm_bindgen::prelude::*;"));
    assert!(out.contains("Generated for wasm32 target"));
}

#[test]
fn nostd_codegen_emits_no_std_prelude() {
    let source = r#"
machine Device {
    state Idle
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    let out = NoStdCodegen::new().generate(&program);
    assert!(out.contains("#![no_std]"));
    assert!(out.contains("extern crate alloc;"));
}

#[test]
fn cffi_codegen_emits_header_and_rust_exports() {
    let source = r#"
machine Door {
    state Closed
    state Open
    transition open: Closed -> Open
    on open() {
        goto Open();
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    let (rust, header) = CffiCodegen::new().generate(&program);
    assert!(header.contains("typedef struct Door Door"));
    assert!(header.contains("int door_open(Door* machine);"));
    assert!(rust.contains("pub unsafe extern \"C\" fn door_open"));
}
