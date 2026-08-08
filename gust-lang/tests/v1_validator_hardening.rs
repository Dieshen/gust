//! The two holes closed for 1.0: unknown generic constructors reaching codegen,
//! and handlers calling past their declared effects.
//!
//! Both passed `gust check` on 0.4.1 and failed later — in generated code the
//! author is told never to edit, or not at all.

use gust_lang::{parse_program_with_errors, validate_program};

fn errors(source: &str) -> Vec<String> {
    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    validate_program(&program, "test.gu", source)
        .errors
        .into_iter()
        .map(|e| e.message)
        .collect()
}

/// Message, note, and help joined — the help line is the useful half of these
/// diagnostics, so it needs asserting too.
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

// ---------------------------------------------------------------------------
// Unknown generic constructors (#133)
// ---------------------------------------------------------------------------

/// The reported case: passed `gust check`, then emitted `HashMap[K, V]` into Go
/// (no such type) and an unimported `HashMap` into Rust.
#[test]
fn hashmap_in_a_type_is_rejected() {
    let d = full_diagnostics("type Bag { lookup: HashMap<String, i64> }");
    assert!(d.contains("unknown generic type 'HashMap'"), "got: {d}");
    assert!(
        d.contains("Gust has no map type"),
        "a map-shaped name deserves a map-specific answer, got: {d}"
    );
}

#[test]
fn known_generics_are_accepted_everywhere() {
    let source = r#"
type Wrapper { items: Vec<String>, maybe: Option<i64>, nested: Vec<Option<String>> }

channel results: Vec<String>

machine Pipe {
    state Idle(queue: Vec<String>)
    state Busy(current: Option<String>)

    transition go: Idle -> Busy

    effect fetch(keys: Vec<String>) -> Result<Option<String>, String>

    on go(ctx) {
        goto Busy(ctx.queue);
    }
}
"#;
    let e = errors(source);
    assert!(
        !e.iter().any(|m| m.contains("unknown generic type")),
        "Vec/Option/Result must stay legal in every position, got: {e:?}"
    );
}

/// Each type position is walked separately, so each needs its own case. A check
/// covering only `type` declarations would pass the test above while leaving
/// states, effects, enums, and channels wide open.
#[test]
fn every_type_position_is_walked() {
    for (label, source) in [
        (
            "state field",
            "machine M { state S(m: HashMap<String, i64>) }",
        ),
        (
            "effect param",
            "machine M { state S effect f(m: HashMap<String, i64>) -> bool }",
        ),
        (
            "effect return",
            "machine M { state S effect f(k: String) -> HashMap<String, i64> }",
        ),
        ("enum payload", "enum E { Full(HashMap<String, i64>) }"),
        ("channel", "channel c: HashMap<String, i64>"),
        (
            "nested inside a known generic",
            "type T { v: Vec<HashMap<String, i64>> }",
        ),
    ] {
        let e = errors(source);
        assert!(
            e.iter()
                .any(|m| m.contains("unknown generic type 'HashMap'")),
            "{label} was not checked, got: {e:?}"
        );
    }
}

#[test]
fn near_miss_names_get_the_right_suggestion() {
    let list = full_diagnostics("type T { v: List<String> }");
    assert!(list.contains("did you mean 'Vec'?"), "got: {list}");

    let maybe = full_diagnostics("type T { v: Maybe<String> }");
    assert!(maybe.contains("did you mean 'Option'?"), "got: {maybe}");

    let set = full_diagnostics("type T { v: HashSet<String> }");
    assert!(set.contains("Gust has no set type"), "got: {set}");
}

/// A machine type parameter is left permissive on purpose — this check exists to
/// stop a plausible-looking name reaching codegen, not to police generics.
#[test]
fn machine_type_parameters_are_not_flagged() {
    let e = errors(
        r#"
machine Box<T> {
    state Empty
    state Full(value: T)
    transition put: Empty -> Full
    on put(value: T) {
        goto Full(value);
    }
}
"#,
    );
    assert!(
        !e.iter().any(|m| m.contains("unknown generic type")),
        "got: {e:?}"
    );
}

// ---------------------------------------------------------------------------
// Free calls — the sandbox boundary
// ---------------------------------------------------------------------------

/// The verified escape: reported "Check passed" on 0.4.1 and emitted
/// `let _ = exit(n);` verbatim into generated Rust.
#[test]
fn a_call_to_an_undeclared_function_is_rejected() {
    let d = full_diagnostics(
        r#"
machine Escape {
    state Start(n: i64)
    state Done(n: i64)
    transition go: Start -> Done
    on go(ctx) {
        let x = exit(ctx.n);
        goto Done(ctx.n);
    }
}
"#,
    );
    assert!(d.contains("call to undeclared function 'exit'"), "got: {d}");
    assert!(d.contains("emitted verbatim"), "got: {d}");
}

/// Calling a declared effect without `perform` is the likely mistake, and gets
/// the exact fix rather than the generic "declare it as an effect".
#[test]
fn calling_a_declared_effect_without_perform_suggests_perform() {
    let d = full_diagnostics(
        r#"
machine Job {
    state New(id: String)
    state Done(id: String)
    transition run: New -> Done
    effect notify(id: String) -> bool
    on run(ctx) {
        let ok = notify(ctx.id);
        goto Done(ctx.id);
    }
}
"#,
    );
    assert!(d.contains("call it as `perform notify(...)`"), "got: {d}");
}

/// Nested one level deep in each traversal arm, so a case can only pass if that
/// arm actually recurses.
#[test]
fn free_calls_are_found_in_every_position() {
    for (label, body) in [
        (
            "if condition",
            "if helper(1) { goto Done(0); }\n        goto Done(1);",
        ),
        (
            "if body",
            "if true { let x = helper(1); goto Done(x); }\n        goto Done(1);",
        ),
        ("goto argument", "goto Done(helper(1));"),
        (
            "perform argument",
            "perform ping(helper(1));\n        goto Done(1);",
        ),
        (
            "binary operand",
            "let x = helper(1) + 2;\n        goto Done(x);",
        ),
    ] {
        let source = format!(
            r#"
machine M {{
    state Start(n: i64)
    state Done(n: i64)
    transition go: Start -> Done
    effect ping(n: i64) -> bool
    on go(ctx) {{
        {body}
    }}
}}
"#
        );
        let e = errors(&source);
        assert!(
            e.iter()
                .any(|m| m.contains("call to undeclared function 'helper'")),
            "{label} was not walked, got: {e:?}"
        );
    }
}

/// `perform` is not a free call and must not be caught by this check.
#[test]
fn perform_is_not_flagged() {
    let e = errors(
        r#"
machine Job {
    state New(id: String)
    state Done(id: String)
    transition run: New -> Done
    effect notify(id: String) -> bool
    on run(ctx) {
        let ok = perform notify(ctx.id);
        if ok {
            goto Done(ctx.id);
        }
        goto Done(ctx.id);
    }
}
"#,
    );
    assert!(
        !e.iter().any(|m| m.contains("undeclared function")),
        "got: {e:?}"
    );
}
