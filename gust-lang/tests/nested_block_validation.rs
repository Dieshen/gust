//! Validator checks must reach inside `if` and `match` bodies.
//!
//! Written in response to mutation testing. Ten survivors over `validator.rs`
//! were the same shape: deleting the `Statement::If` or `Statement::Match` arm
//! from a walker — in `validate_goto_arity`, `check_match_exhaustiveness`,
//! `collect_let_bindings`, `collect_lossy_result_bindings`,
//! `check_expr_perform_arity`, and `destructures_used_error`. Every one left
//! the suite green.
//!
//! The recursion is *correct*; this was verified by hand before writing the
//! tests, so what follows is a coverage gap rather than a bug. That distinction
//! matters for how much to trust it: nothing was protecting behaviour every
//! user depends on. A `goto` inside a bare `if` is not an exotic shape — it is
//! the ordinary way to write an early exit, and it is the exact shape #136
//! broke in the Go backend.
//!
//! Every fixture here puts the defect **only** in a nested position. A test
//! whose bad `goto` also appears at the top level would pass with the nested
//! arm deleted, which is how this gap survived.
//!
//! # Not covered, and why
//!
//! `check_expr_perform_arity`'s nested arms are still uncovered. Writing the
//! test showed the check does not fire for a wrong-arity `perform` used as a
//! `goto` argument **at the top level either**, so there is no nested case to
//! assert — the survivor is a genuine gap in the check, not in its recursion,
//! and asserting the nested form would encode a wish rather than a behaviour.
//!
//! Two fixtures here were wrong on the first attempt in exactly that way, and
//! both were caught by checking whether the top-level form was diagnosed before
//! believing the nested one was a bug. Exhaustiveness, for instance, needs a
//! *bare* field scrutinee and *qualified* patterns — `match tier` with
//! `Tier::Fast`, not `match ctx.tier` with `Fast`.

use gust_lang::{parse_program_with_errors, validate_program};

fn errors_for(source: &str) -> Vec<String> {
    let program = parse_program_with_errors(source, "nested.gu").expect("fixture should parse");
    validate_program(&program, "nested.gu", source)
        .errors
        .into_iter()
        .map(|e| e.message)
        .collect()
}

fn warnings_for(source: &str) -> Vec<String> {
    let program = parse_program_with_errors(source, "nested.gu").expect("fixture should parse");
    validate_program(&program, "nested.gu", source)
        .warnings
        .into_iter()
        .map(|w| w.message)
        .collect()
}

fn assert_reports(found: &[String], needle: &str, where_: &str) {
    assert!(
        found.iter().any(|m| m.contains(needle)),
        "expected a diagnostic containing {needle:?} for a defect inside {where_}; got {found:?}"
    );
}

/// The only wrong-arity `goto` is inside a bare `if`.
#[test]
fn goto_arity_is_checked_inside_an_if() {
    let source = r#"
machine M {
    state A(n: i64)
    state B(x: i64, y: String)
    transition go: A -> B
    on go(ctx) {
        if ctx.n > 0 {
            goto B(1);
        }
        goto B(2, "ok");
    }
}
"#;
    assert_reports(&errors_for(source), "expects 2 argument", "an if block");
}

/// The only wrong-arity `goto` is inside an `else` block.
#[test]
fn goto_arity_is_checked_inside_an_else() {
    let source = r#"
machine M {
    state A(n: i64)
    state B(x: i64, y: String)
    transition go: A -> B
    on go(ctx) {
        if ctx.n > 0 {
            goto B(1, "ok");
        } else {
            goto B(2);
        }
    }
}
"#;
    assert_reports(&errors_for(source), "expects 2 argument", "an else block");
}

/// The only wrong-arity `goto` is inside a `match` arm.
#[test]
fn goto_arity_is_checked_inside_a_match_arm() {
    let source = r#"
machine M {
    state A(n: i64)
    state Done(x: i64, y: String)
    state Failed(reason: String)
    transition go: A -> Done | Failed
    effect run(n: i64) -> Result<String, String>
    on go(ctx) {
        let outcome = perform run(ctx.n);
        match outcome {
            Ok(value) => {
                goto Done(1);
            }
            Err(reason) => {
                goto Failed(reason);
            }
        }
    }
}
"#;
    assert_reports(&errors_for(source), "expects 2 argument", "a match arm");
}

/// A non-exhaustive `match` nested inside an `if`.
///
/// `match_covers_all_enum_variants` could be replaced with `true` outright
/// without failing anything, so this asserts the check exists at all as well as
/// that it reaches nested blocks.
#[test]
fn match_exhaustiveness_is_checked_inside_an_if() {
    let source = r#"
enum Tier { Fast, Slow, Bulk }

machine M {
    state A(n: i64, tier: Tier)
    state B
    transition go: A -> B
    on go(ctx) {
        if ctx.n > 0 {
            match tier {
                Tier::Fast => {
                    goto B();
                }
                Tier::Slow => {
                    goto B();
                }
            }
        }
        goto B();
    }
}
"#;
    assert_reports(&warnings_for(source), "non-exhaustive", "an if block");
}

/// An unused `let` whose only occurrence is inside a `match` arm.
#[test]
fn unused_bindings_are_found_inside_a_match_arm() {
    let source = r#"
machine M {
    state A(n: i64)
    state Done
    state Failed(reason: String)
    transition go: A -> Done | Failed
    effect run(n: i64) -> Result<String, String>
    effect audit(id: String) -> String
    on go(ctx) {
        let outcome = perform run(ctx.n);
        match outcome {
            Ok(value) => {
                let ignored = perform audit(value);
                goto Done();
            }
            Err(reason) => {
                goto Failed(reason);
            }
        }
    }
}
"#;
    assert_reports(&warnings_for(source), "unused binding", "a match arm");
}

/// An unused `let` whose only occurrence is inside an `if`.
#[test]
fn unused_bindings_are_found_inside_an_if() {
    let source = r#"
machine M {
    state A(n: i64)
    state B
    transition go: A -> B
    effect audit(id: i64) -> String
    on go(ctx) {
        if ctx.n > 0 {
            let ignored = perform audit(ctx.n);
            goto B();
        }
        goto B();
    }
}
"#;
    assert_reports(&warnings_for(source), "unused binding", "an if block");
}
