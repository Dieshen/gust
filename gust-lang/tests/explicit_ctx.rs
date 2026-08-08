//! The from-state accessor is identified by syntax, not by the compiler failing
//! to recognise a type name.
//!
//! Until 1.0, `detect_ctx_param` keyed off `!known_types.contains(type_name)`.
//! That made "the compiler does not know this name" load-bearing syntax, with
//! three consequences: a typo in a parameter's type silently deleted the
//! parameter, a machine's own generic parameters had to be threaded in specially
//! or `on put(value: T)` lost its argument, and **every future builtin type name
//! would silently change the signature of handlers that already compiled**.
//!
//! The last one is why this mattered for 1.0 rather than 1.x: it makes growing
//! the type system a breaking change against source nobody edited.

use gust_lang::{GoCodegen, RustCodegen, parse_program_with_errors, validate_program};

fn errors(source: &str) -> Vec<String> {
    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    validate_program(&program, "test.gu", source)
        .errors
        .into_iter()
        .map(|e| e.message)
        .collect()
}

fn full_diagnostics(source: &str) -> String {
    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    validate_program(&program, "test.gu", source)
        .errors
        .iter()
        .map(|e| {
            format!(
                "{}|{}|{}",
                e.message,
                e.note.clone().unwrap_or_default(),
                e.help.clone().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn rust_of(source: &str) -> String {
    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    RustCodegen::new().generate(&program)
}

// ---------------------------------------------------------------------------
// The acceptance test
// ---------------------------------------------------------------------------

/// **The** test for this change: a name the compiler did not previously know
/// must not change generated output when it becomes known.
///
/// Before 1.0 this was impossible to satisfy — a parameter typed `Widget` was a
/// ctx accessor while `Widget` was unknown, and a real parameter the moment a
/// `type Widget` declaration appeared. The same handler, two signatures, no
/// diagnostic.
///
/// The check is now stronger than "the list did not change": there is no list.
/// `BUILTIN_TYPES`, `collect_known_types`, and `machine_known_types` were
/// deleted, because nothing reads them any more.
#[test]
fn declaring_a_type_does_not_change_a_handler_signature() {
    let without_decl = r#"
machine M {
    state Start(n: i64)
    state Done(n: i64)

    transition go: Start -> Done

    on go(ctx) {
        goto Done(ctx.n);
    }
}
"#;
    // Same machine, but now `Widget` is a declared type in the program. Under
    // the old rule, introducing a declaration anywhere could flip a parameter's
    // role; the handler below deliberately does not mention `Widget` at all.
    let with_decl = r#"
type Widget { label: String }

machine M {
    state Start(n: i64)
    state Done(n: i64)

    transition go: Start -> Done

    on go(ctx) {
        goto Done(ctx.n);
    }
}
"#;

    let a = rust_of(without_decl);
    let b = rust_of(with_decl);

    // Strip the extra type declaration so the machine halves are comparable.
    let b_machine = b
        .split("pub enum MState")
        .nth(1)
        .expect("state enum present in output");
    let a_machine = a
        .split("pub enum MState")
        .nth(1)
        .expect("state enum present in output");

    assert_eq!(
        a_machine, b_machine,
        "declaring an unrelated type changed the generated machine"
    );
}

/// A generic machine's type parameter is an ordinary parameter type, and needs
/// no special threading to stay one.
#[test]
fn a_generic_type_parameter_is_a_real_parameter() {
    let source = r#"
machine Box<T> {
    state Empty
    state Full(value: T)

    transition put: Empty -> Full

    on put(value: T) {
        goto Full(value);
    }
}
"#;
    let rust = rust_of(source);
    assert!(
        rust.contains("value: T"),
        "the typed parameter must survive into the signature, got:\n{rust}"
    );
    assert!(errors(source).is_empty(), "{:?}", errors(source));
}

// ---------------------------------------------------------------------------
// The two rules
// ---------------------------------------------------------------------------

#[test]
fn an_untyped_ctx_parameter_is_the_from_state_accessor() {
    let source = r#"
machine Boss {
    state Idle(job: String)
    state Running(current: String)

    transition begin: Idle -> Running

    on begin(ctx) {
        goto Running(ctx.job);
    }
}
"#;
    assert!(errors(source).is_empty(), "{:?}", errors(source));

    let rust = rust_of(source);
    assert!(
        !rust.contains("ctx"),
        "ctx is an accessor, not an argument; it must not reach the signature:\n{rust}"
    );
    assert!(
        rust.contains("job"),
        "ctx.job should lower to the source-state field:\n{rust}"
    );
}

/// The migration diagnostic. `on begin(ctx: BeginCtx)` was the documented idiom
/// and `BeginCtx` never existed, so this needs to say so precisely rather than
/// falling through to "unknown type".
#[test]
fn annotating_ctx_is_an_error_that_names_the_fix() {
    let d = full_diagnostics(
        r#"
machine Boss {
    state Idle(job: String)
    state Running(current: String)
    transition begin: Idle -> Running
    on begin(ctx: BeginCtx) {
        goto Running(ctx.job);
    }
}
"#,
    );
    assert!(
        d.contains("the 'ctx' parameter must not have a type annotation"),
        "got: {d}"
    );
    assert!(d.contains("on begin(ctx)"), "got: {d}");
    assert!(
        d.contains("that type never existed"),
        "the diagnostic should say why, got: {d}"
    );
}

#[test]
fn an_untyped_parameter_not_named_ctx_is_an_error() {
    let d = full_diagnostics(
        r#"
machine M {
    state Start(n: i64)
    state Done(n: i64)
    transition go: Start -> Done
    on go(thing) {
        goto Done(0);
    }
}
"#,
    );
    assert!(
        d.contains("handler parameter 'thing' has no type"),
        "got: {d}"
    );
    assert!(d.contains("only 'ctx'"), "got: {d}");
}

#[test]
fn a_typed_parameter_alongside_ctx_is_fine() {
    let source = r#"
machine M {
    state Start(n: i64)
    state Done(n: i64)

    transition go: Start -> Done

    on go(ctx, extra: i64) {
        goto Done(extra);
    }
}
"#;
    assert!(errors(source).is_empty(), "{:?}", errors(source));

    let rust = rust_of(source);
    assert!(
        rust.contains("extra: i64"),
        "the typed parameter must survive:\n{rust}"
    );
}

// ---------------------------------------------------------------------------
// Both backends, and the formatter
// ---------------------------------------------------------------------------

#[test]
fn go_output_also_drops_the_accessor_from_the_signature() {
    let source = r#"
machine Boss {
    state Idle(job: String)
    state Running(current: String)

    transition begin: Idle -> Running

    on begin(ctx) {
        goto Running(ctx.job);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    let go = GoCodegen::new().generate(&program, "boss");
    assert!(
        go.contains("func (m *Boss) Begin("),
        "transition method should exist:\n{go}"
    );
    assert!(
        !go.contains("ctx Ctx") && !go.contains("ctx BeginCtx"),
        "the accessor must not become a Go parameter:\n{go}"
    );
}

/// `gust fmt` must round-trip the new form, or formatting silently reintroduces
/// an annotation — or drops the parameter.
#[test]
fn the_formatter_round_trips_an_untyped_ctx() {
    let source = r#"machine Boss {
    state Idle(job: String)
    state Running(current: String)

    transition begin: Idle -> Running

    on begin(ctx) {
        goto Running(ctx.job);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    let formatted = gust_lang::format_program(&program);
    assert!(
        formatted.contains("on begin(ctx)"),
        "formatter must preserve the untyped accessor, got:\n{formatted}"
    );

    let reparsed =
        parse_program_with_errors(&formatted, "test.gu").expect("formatted source should reparse");
    assert!(
        reparsed.machines[0].handlers[0].params[0].ty.is_none(),
        "round-trip must keep the parameter untyped"
    );
}
