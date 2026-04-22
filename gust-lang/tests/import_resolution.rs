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

// === Regression for issue #66 ==============================================
// `use std::*` is a Gust-virtual namespace for stdlib machines/types. Neither
// Rust nor Go codegen may emit it literally, because Rust's `std` crate has
// no matching item and `std/Foo` is not a valid Go module path. Stdlib
// sources are compiled separately by the consumer's build pipeline.

#[test]
fn std_imports_stripped_from_rust_codegen() {
    let source = r#"
use std::EngineFailure;

machine Worker {
    state Idle
    state Done

    transition finish: Idle -> Done

    on finish() {
        goto Done();
    }
}
"#;

    let program = parse_program(source).expect("source should parse");
    let output = RustCodegen::new().generate(&program);

    assert!(
        !output.contains("use std::EngineFailure"),
        "std::* imports must not leak into generated Rust (collides with std crate). \
         Generated output:\n{output}"
    );
    // Sanity: the rest of the prelude is still there.
    assert!(output.contains("use gust_runtime::prelude::*;"));
}

#[test]
fn std_imports_stripped_from_go_codegen() {
    let source = r#"
use std::EngineFailure;

machine Worker {
    state Idle
    state Done

    transition finish: Idle -> Done

    on finish() {
        goto Done();
    }
}
"#;

    let program = parse_program(source).expect("source should parse");
    let output = GoCodegen::new().generate(&program, "testpkg");

    assert!(
        !output.contains("\"std/EngineFailure\""),
        "std::* imports must not leak into Go import list. Generated output:\n{output}"
    );
    assert!(
        !output.contains("\"std\""),
        "Gust's `std` namespace must not map to a Go `std` import. Generated output:\n{output}"
    );
}

#[test]
fn non_std_imports_still_emitted_after_std_filter() {
    // Mixing `std::*` with a real import: the real import must survive.
    let source = r#"
use std::EngineFailure;
use crate::domain::OrderId;

machine Worker {
    state Idle
    state Done

    transition finish: Idle -> Done

    on finish() {
        goto Done();
    }
}
"#;

    let program = parse_program(source).expect("source should parse");
    let rust_output = RustCodegen::new().generate(&program);

    assert!(!rust_output.contains("use std::EngineFailure"));
    assert!(rust_output.contains("use crate::domain::OrderId;"));
}
