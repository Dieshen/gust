use gust_lang::{CffiCodegen, NoStdCodegen, WasmCodegen, parse_program_with_errors};

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
    assert!(out.contains("use wasm_bindgen_futures::future_to_promise;"));
    assert!(out.contains("#[wasm_bindgen]"));
    assert!(out.contains("pub trait GustWasmEffectAdapter"));
    assert!(out.contains("Promise"));
}

#[test]
fn nostd_codegen_emits_no_std_prelude() {
    let source = r#"
machine Device {
    state Idle(name: String, values: Vec<i64>)
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    let out = NoStdCodegen::new().generate(&program);
    assert!(out.contains("#![no_std]"));
    assert!(out.contains("use heapless::{String as HString, Vec as HVec};"));
    assert!(out.contains("name: HString<64>"));
    assert!(out.contains("values: HVec<i64, 16>"));
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
    assert!(header.contains("typedef struct DoorHandle DoorHandle;"));
    assert!(header.contains("int door_open(DoorHandle* handle);"));
    assert!(header.contains("typedef enum DoorState"));
    assert!(rust.contains("#[repr(C)]"));
    assert!(rust.contains("pub unsafe extern \"C\" fn door_open"));
    assert!(rust.contains("return -1;"));
    assert!(rust.contains("return -2;"));
}
