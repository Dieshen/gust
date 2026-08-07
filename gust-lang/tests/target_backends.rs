use gust_lang::{CffiCodegen, parse_program_with_errors};

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
