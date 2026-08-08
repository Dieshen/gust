//! `use` is a Gust-level import. It emits nothing to any backend.
//!
//! Until 1.0 it meant two different things. `use std::Foo` was a Gust-virtual
//! stdlib import that emitted nothing; anything else was passed through as a
//! *host-language* import — a real Rust `use` statement, a real Go import.
//!
//! That second meaning is the keyword collision the 1.0 de-landmining had to
//! resolve, because the module system planned for 1.x needs `use` to mean
//! "import from another `.gu`", and a keyword cannot change meaning inside a
//! stability promise.
//!
//! It had also stopped working. Handlers may only call declared effects as of
//! 1.0, and qualified calls with arguments never parsed, so nothing in a `.gu`
//! could reference an imported symbol. On Go that produced an import which was
//! by construction unused — output that does not compile.
//!
//! So `use` keeps only its Gust meaning: it names a type declared elsewhere, so
//! the validator accepts it, and the consumer's build pipeline is responsible
//! for putting that declaration in the same module or package.

use gust_lang::{GoCodegen, RustCodegen, parse_program, validate_program};

fn rust_of(source: &str) -> String {
    let program = parse_program(source).expect("source should parse");
    RustCodegen::new().generate(&program)
}

fn go_of(source: &str) -> String {
    let program = parse_program(source).expect("source should parse");
    GoCodegen::new().generate(&program, "testpkg")
}

const WITH_IMPORTS: &str = r#"
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

#[test]
fn no_use_declaration_reaches_generated_rust() {
    let output = rust_of(WITH_IMPORTS);

    assert!(
        !output.contains("use std::EngineFailure"),
        "the Gust `std` namespace has no matching item in Rust's std crate:\n{output}"
    );
    assert!(
        !output.contains("use crate::domain::OrderId"),
        "a `use` must not become a host import:\n{output}"
    );

    // The prelude is unaffected. It deliberately does not import
    // `gust_runtime::prelude`, which was inert and forced a dependency on
    // machines that never touch the runtime.
    assert!(output.contains("use serde::{Serialize, Deserialize};"));
    assert!(!output.contains("use gust_runtime::prelude::*;"));
}

#[test]
fn no_use_declaration_reaches_generated_go() {
    let output = go_of(WITH_IMPORTS);

    for forbidden in [
        "\"std/EngineFailure\"",
        "\"std\"",
        "\"crate/domain/OrderId\"",
    ] {
        assert!(
            !output.contains(forbidden),
            "{forbidden} must not appear in the import list:\n{output}"
        );
    }

    // The backend's own imports are still there.
    assert!(output.contains("\"encoding/json\""));
    assert!(output.contains("\"fmt\""));
}

/// The concrete regression: an import nothing can reference is one `go vet`
/// rejects outright. This is why the passthrough could not simply be left
/// alone.
#[test]
fn a_host_shaped_use_no_longer_produces_an_unused_go_import() {
    let output = go_of(
        r#"
use os;

machine M {
    state Start(n: i64)
    state Done(n: i64)

    transition go: Start -> Done

    on go(ctx) {
        goto Done(ctx.n);
    }
}
"#,
    );

    assert!(
        !output.contains("\"os\""),
        "`use os;` emitted an import that nothing references, which Go rejects \
         as \"imported and not used\":\n{output}"
    );
}

/// The other half of the design: because `use` no longer emits anything, it is
/// free to mean "this type is declared elsewhere" — which is what makes
/// rejecting unknown type names workable.
#[test]
fn a_used_name_is_accepted_as_a_type() {
    let source = r#"
use std::EngineFailure;

machine Job {
    state Running(id: String)
    state Failed(failure: EngineFailure)

    transition abort: Running -> Failed

    effect classify(id: String) -> EngineFailure

    on abort(ctx) {
        let f = perform classify(ctx.id);
        goto Failed(f);
    }
}
"#;
    let program = parse_program(source).expect("source should parse");
    let report = validate_program(&program, "test.gu", source);
    let messages: Vec<&str> = report.errors.iter().map(|e| e.message.as_str()).collect();

    assert!(
        messages.is_empty(),
        "an imported type must be accepted wherever a declared one would be, got: {messages:?}"
    );
}

/// Without the import, the same source is rejected — otherwise `use` would be
/// decorative and the check meaningless.
#[test]
fn the_same_type_without_an_import_is_rejected() {
    let source = r#"
machine Job {
    state Running(id: String)
    state Failed(failure: EngineFailure)

    transition abort: Running -> Failed

    on abort(ctx) {
        goto Failed(ctx.id);
    }
}
"#;
    let program = parse_program(source).expect("source should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report
            .errors
            .iter()
            .any(|e| e.message.contains("unknown type 'EngineFailure'")),
        "got: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}
