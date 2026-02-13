use gust_lang::{parse_program, GoCodegen, RustCodegen};

#[test]
fn import_resolution_rust_emits_use_paths() {
    let source = r#"
use crate::models::Order;
use crate::types::Money;

type Amount {
    cents: i64,
}

machine Processor {
    state Pending(order: Order)
    state Done(amount: Amount)

    transition complete: Pending -> Done

    on complete() {
        goto Done(Amount(0));
    }
}
"#;

    let program = parse_program(source).expect("source should parse");
    let output = RustCodegen::new().generate(&program);

    assert!(output.contains("use crate::models::Order;"));
    assert!(output.contains("use crate::types::Money;"));
    assert!(output.contains("use gust_runtime::prelude::*;"));
}

#[test]
fn import_resolution_go_emits_use_paths() {
    let source = r#"
use github::com::acme::payments;

machine Processor {
    state Pending
    state Done

    transition complete: Pending -> Done

    on complete() {
        goto Done();
    }
}
"#;

    let program = parse_program(source).expect("source should parse");
    let output = GoCodegen::new().generate(&program, "testpkg");

    assert!(output.contains("\"github.com/acme/payments\""));
    assert!(output.contains("\"encoding/json\""));
    assert!(output.contains("\"fmt\""));
}
