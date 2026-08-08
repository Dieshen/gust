//! Warnings are asserted here, positively and negatively.
//!
//! Written in response to mutation testing. Running `cargo mutants` over
//! `validator.rs` showed the survivors clustering in one place: warning
//! generation. Inverting the "transition has no handler" condition, inverting
//! "unused effect", and breaking the incoming-transition counter that drives
//! unreachable-state detection all left the suite green. The error paths were
//! well covered; the warning paths were barely covered at all.
//!
//! That asymmetry is easy to arrive at — errors fail a build, so tests get
//! written for them — and it is the wrong way round for the things users
//! actually lean on day to day. "You declared an effect and never performed it"
//! is usually a rename that missed a call site; "this transition has no
//! handler" is usually a typo. A warning that stops firing is silent by
//! definition.
//!
//! Each case asserts **both directions**. Only checking that a warning fires
//! lets a validator that warns unconditionally pass, which is the same defect
//! wearing a different hat.

use gust_lang::{parse_program_with_errors, validate_program};

fn warnings_for(source: &str) -> Vec<String> {
    let program = parse_program_with_errors(source, "warnings.gu").expect("fixture should parse");
    let report = validate_program(&program, "warnings.gu", source);
    assert!(
        report.errors.is_empty(),
        "fixture should be error-free; got: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
    report.warnings.into_iter().map(|w| w.message).collect()
}

fn assert_warns(source: &str, needle: &str) {
    let warnings = warnings_for(source);
    assert!(
        warnings.iter().any(|w| w.contains(needle)),
        "expected a warning containing {needle:?}, got {warnings:?}"
    );
}

fn assert_does_not_warn(source: &str, needle: &str) {
    let warnings = warnings_for(source);
    assert!(
        !warnings.iter().any(|w| w.contains(needle)),
        "unexpected warning containing {needle:?}: {warnings:?}"
    );
}

const WITH_HANDLER: &str = r#"
machine M {
    state A
    state B
    transition go: A -> B
    on go() {
        goto B();
    }
}
"#;

/// The same machine with the handler removed — nothing else differs.
const WITHOUT_HANDLER: &str = r#"
machine M {
    state A
    state B
    transition go: A -> B
}
"#;

#[test]
fn a_transition_with_no_handler_warns() {
    assert_warns(WITHOUT_HANDLER, "has no handler");
}

#[test]
fn a_transition_with_a_handler_does_not_warn() {
    assert_does_not_warn(WITH_HANDLER, "has no handler");
}

const EFFECT_USED: &str = r#"
machine M {
    state A
    state B
    transition go: A -> B
    effect audit(id: String) -> ()
    on go() {
        perform audit("x");
        goto B();
    }
}
"#;

const EFFECT_UNUSED: &str = r#"
machine M {
    state A
    state B
    transition go: A -> B
    effect audit(id: String) -> ()
    on go() {
        goto B();
    }
}
"#;

#[test]
fn a_declared_but_never_performed_effect_warns() {
    assert_warns(EFFECT_UNUSED, "unused effect");
}

#[test]
fn a_performed_effect_does_not_warn() {
    assert_does_not_warn(EFFECT_USED, "unused effect");
}

/// `Orphan` is targeted by no transition, so it can never be entered.
const UNREACHABLE_STATE: &str = r#"
machine M {
    state A
    state B
    state Orphan
    transition go: A -> B
    on go() {
        goto B();
    }
}
"#;

#[test]
fn a_state_no_transition_targets_warns() {
    assert_warns(UNREACHABLE_STATE, "unreachable state");
}

/// Every state here is either the initial state or a transition target.
///
/// This is the direction that catches a broken incoming-transition counter: if
/// the counter stops incrementing, every state looks unreachable and this fires
/// spuriously. Asserting only the positive case above would miss that.
#[test]
fn reachable_states_do_not_warn() {
    assert_does_not_warn(WITH_HANDLER, "unreachable state");
}

/// A state reached only via the *second* target of a multi-target transition.
///
/// Counting only the first target would leave `Failed` looking unreachable.
#[test]
fn every_target_of_a_multi_target_transition_counts_as_reachable() {
    let source = r#"
machine M {
    state A(ok: bool)
    state Done
    state Failed
    transition go: A -> Done | Failed
    on go(ctx) {
        if ctx.ok {
            goto Done();
        }
        goto Failed();
    }
}
"#;
    assert_does_not_warn(source, "unreachable state");
}

#[test]
fn is_ok_reports_true_when_there_are_no_errors() {
    let program = parse_program_with_errors(WITH_HANDLER, "warnings.gu").expect("should parse");
    let report = validate_program(&program, "warnings.gu", WITH_HANDLER);
    assert!(report.is_ok(), "a valid program should report is_ok()");
}

/// `is_ok` is about errors, not warnings — a program that only warns is still
/// ok, and conflating the two would make `gust check` fail on a lint.
#[test]
fn is_ok_stays_true_when_there_are_only_warnings() {
    let program = parse_program_with_errors(EFFECT_UNUSED, "warnings.gu").expect("should parse");
    let report = validate_program(&program, "warnings.gu", EFFECT_UNUSED);
    assert!(!report.warnings.is_empty(), "fixture should warn");
    assert!(report.is_ok(), "warnings alone must not make is_ok() false");
}

#[test]
fn is_ok_reports_false_when_there_is_an_error() {
    let source = r#"
machine M {
    state A
    transition go: A -> Nowhere
    on go() {
        goto Nowhere();
    }
}
"#;
    let program = parse_program_with_errors(source, "warnings.gu").expect("should parse");
    let report = validate_program(&program, "warnings.gu", source);
    assert!(!report.errors.is_empty(), "fixture should error");
    assert!(
        !report.is_ok(),
        "an erroring program must not report is_ok()"
    );
}
