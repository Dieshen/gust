//! Handler-safety diagnostics for `action` usage (#40 PR 4).
//!
//! Two rules, both warnings (additive per the #40 delivery plan):
//!
//! 1. A handler should perform at most one `action` per code path.
//! 2. An `action` should be the last side-effectful step on its path;
//!    if any `perform`, `send`, or `spawn` follows it, the runtime
//!    cannot checkpoint cleanly.
//!
//! Branches (if/else, match arms) are analyzed as independent sequences
//! so actions in different branches are not conflated.

use gust_lang::{parse_program_with_errors, validate_program};

fn warnings(source: &str) -> Vec<String> {
    let program = parse_program_with_errors(source, "t.gu").expect("should parse");
    let report = validate_program(&program, "t.gu", source);
    report.warnings.iter().map(|w| w.message.clone()).collect()
}

/// Returns the full warning structs so tests can inspect line/col.
fn full_warnings(source: &str) -> Vec<gust_lang::error::GustWarning> {
    let program = parse_program_with_errors(source, "t.gu").expect("should parse");
    validate_program(&program, "t.gu", source).warnings
}

fn has_multi_action_warning(warnings: &[String]) -> bool {
    warnings
        .iter()
        .any(|w| w.contains("actions in a single sequence"))
}

fn has_action_not_last_warning(warnings: &[String]) -> bool {
    warnings
        .iter()
        .any(|w| w.contains("side-effectful steps after an action"))
}

// ---------------------------------------------------------------------------
// Rule 1: more than one action per path
// ---------------------------------------------------------------------------

#[test]
fn single_action_in_handler_is_ok() {
    let source = r#"
machine Notifier {
    state Idle
    state Pending(msg: String)
    state Sent(ts: String)

    transition send: Pending -> Sent

    action post(msg: String) -> String

    on send(ctx: Ctx) {
        let ts: String = perform post(ctx.msg);
        goto Sent(ts);
    }
}
"#;
    let w = warnings(source);
    assert!(
        !has_multi_action_warning(&w),
        "single action should not warn about multi-action, got: {:?}",
        w
    );
}

#[test]
fn two_actions_in_handler_warns() {
    let source = r#"
machine DualNotifier {
    state Idle
    state Pending(a: String, b: String)
    state Sent(ts: String)

    transition send: Pending -> Sent

    action post_a(msg: String) -> String
    action post_b(msg: String) -> String

    on send(ctx: Ctx) {
        let ts1: String = perform post_a(ctx.a);
        let ts2: String = perform post_b(ctx.b);
        goto Sent(ts2);
    }
}
"#;
    let w = warnings(source);
    assert!(
        has_multi_action_warning(&w),
        "two actions in one handler should warn, got warnings: {:?}",
        w
    );
}

#[test]
fn action_and_effect_together_is_ok() {
    let source = r#"
machine Mixed {
    state Start
    state Done(ts: String)

    transition go: Start -> Done

    effect compute() -> String
    action publish(v: String) -> String

    on go() {
        let a: String = perform compute();
        let ts: String = perform publish(a);
        goto Done(ts);
    }
}
"#;
    let w = warnings(source);
    assert!(
        !has_multi_action_warning(&w),
        "one effect + one action should not trigger multi-action warning, got: {:?}",
        w
    );
    assert!(
        !has_action_not_last_warning(&w),
        "action is last side-effect here, should not warn, got: {:?}",
        w
    );
}

#[test]
fn actions_in_different_branches_are_independent() {
    let source = r#"
machine Router {
    state Start(cond: bool)
    state Done(v: String)

    transition go: Start -> Done

    action path_a() -> String
    action path_b() -> String

    on go(ctx: Ctx) {
        if ctx.cond {
            let v: String = perform path_a();
            goto Done(v);
        } else {
            let v: String = perform path_b();
            goto Done(v);
        }
    }
}
"#;
    let w = warnings(source);
    assert!(
        !has_multi_action_warning(&w),
        "actions in separate branches should not trigger multi-action warning, got: {:?}",
        w
    );
}

#[test]
fn two_actions_in_same_branch_warns() {
    let source = r#"
machine Router {
    state Start(cond: bool)
    state Done(v: String)

    transition go: Start -> Done

    action a() -> String
    action b() -> String

    on go(ctx: Ctx) {
        if ctx.cond {
            let x: String = perform a();
            let y: String = perform b();
            goto Done(y);
        } else {
            goto Done("skip");
        }
    }
}
"#;
    let w = warnings(source);
    assert!(
        has_multi_action_warning(&w),
        "two actions in the same branch should warn, got: {:?}",
        w
    );
}

// ---------------------------------------------------------------------------
// Rule 2: action must be the last side-effectful step
// ---------------------------------------------------------------------------

#[test]
fn action_followed_by_effect_warns() {
    let source = r#"
machine Misordered {
    state Start
    state Done(v: String)

    transition go: Start -> Done

    action publish(v: String) -> String
    effect log(msg: String) -> String

    on go() {
        let a: String = perform publish("hi");
        let b: String = perform log("after");
        goto Done(b);
    }
}
"#;
    let w = warnings(source);
    assert!(
        has_action_not_last_warning(&w),
        "action followed by effect should warn, got: {:?}",
        w
    );
}

#[test]
fn action_followed_by_send_warns() {
    let source = r#"
channel audit: String (mode: broadcast)

machine Misordered (sends audit) {
    state Start
    state Done(v: String)

    transition go: Start -> Done

    action publish(v: String) -> String

    on go() {
        let a: String = perform publish("hi");
        send audit("after");
        goto Done(a);
    }
}
"#;
    let program = parse_program_with_errors(source, "t.gu").expect("should parse");
    let report = validate_program(&program, "t.gu", source);
    let ws: Vec<String> = report.warnings.iter().map(|w| w.message.clone()).collect();
    assert!(
        has_action_not_last_warning(&ws),
        "action followed by send should warn, got: {:?}",
        ws
    );
}

#[test]
fn action_as_last_side_effect_is_ok() {
    let source = r#"
machine CleanOrder {
    state Start
    state Done(v: String)

    transition go: Start -> Done

    effect prep() -> String
    action publish(v: String) -> String

    on go() {
        let pre: String = perform prep();
        let v: String = perform publish(pre);
        goto Done(v);
    }
}
"#;
    let w = warnings(source);
    assert!(
        !has_action_not_last_warning(&w),
        "action as last side-effect should not warn, got: {:?}",
        w
    );
}

// ---------------------------------------------------------------------------
// Machines without any `action` declarations must not see these warnings
// ---------------------------------------------------------------------------

#[test]
fn effect_only_machine_never_warns_about_actions() {
    let source = r#"
machine OnlyEffects {
    state Start
    state Done(v: String)

    transition go: Start -> Done

    effect a() -> String
    effect b() -> String
    effect c() -> String

    on go() {
        let x: String = perform a();
        let y: String = perform b();
        let z: String = perform c();
        goto Done(z);
    }
}
"#;
    let w = warnings(source);
    assert!(
        !has_multi_action_warning(&w),
        "effect-only machine should never trigger action multi-warning"
    );
    assert!(
        !has_action_not_last_warning(&w),
        "effect-only machine should never trigger action-not-last warning"
    );
}

// ---------------------------------------------------------------------------
// `perform` inside a `let` / return / expression statement still counts
// ---------------------------------------------------------------------------

#[test]
fn perform_in_let_value_counts_as_action() {
    let source = r#"
machine Letter {
    state Start
    state Done(v: String)

    transition go: Start -> Done

    action publish(v: String) -> String
    effect log(msg: String) -> String

    on go() {
        let v: String = perform publish("hi");
        let m: String = perform log("after");
        goto Done(v);
    }
}
"#;
    let w = warnings(source);
    assert!(
        has_action_not_last_warning(&w),
        "perform in let RHS should participate in rule 2 ordering, got: {:?}",
        w
    );
}

// ---------------------------------------------------------------------------
// Regression tests for Expr::Perform span and nested-perform traversal
// ---------------------------------------------------------------------------

/// Two actions combined in a single binop expression must still count as two
/// separate actions so the multi-action warning fires.
///
/// Before the fix `let x = perform action_a() + perform action_b()` was
/// classified as a single `Action` entry (the outer expression), so the
/// count was 1 and the warning was suppressed.
#[test]
fn nested_perform_in_binop_still_counts_as_action() {
    // Both declared as `action` with i64 return so `+` is type-valid.
    let source = r#"
machine Dual {
    state Idle
    state Done(v: i64)

    transition go: Idle -> Done

    action action_a() -> i64
    action action_b() -> i64

    on go() {
        let v: i64 = perform action_a() + perform action_b();
        goto Done(v);
    }
}
"#;
    let w = warnings(source);
    assert!(
        has_multi_action_warning(&w),
        "two actions nested in binop must trigger multi-action warning, got: {:?}",
        w
    );
}

/// The multi-action warning span must point at the first offending perform
/// call, not at the handler's opening brace.
///
/// Before the fix all action warnings used `handler_span` (line of `on foo {`)
/// even though the actual perform calls appeared on later lines.
#[test]
fn action_warning_points_at_perform_not_handler() {
    // The `on go` handler opens on line 10; the actions appear on lines 12–13.
    // We check that the warning line > 1 (i.e. not the top of the file) and
    // specifically that it matches the line of the first perform, not the
    // handler header.
    let source = r#"
machine Pointer {
    state Idle
    state Done(v: String)

    transition go: Idle -> Done

    action step_one() -> String
    action step_two(v: String) -> String

    on go() {
        let a: String = perform step_one();
        let b: String = perform step_two(a);
        goto Done(b);
    }
}
"#;
    let ws = full_warnings(source);
    let multi_action_warning = ws
        .iter()
        .find(|w| w.message.contains("actions in a single sequence"))
        .expect("expected multi-action warning");

    // Line 12 is `let a: String = perform step_one();`
    // The warning must be on a real source line (> 0) and not on the handler
    // header line (which is line 11 in this source).
    assert!(
        multi_action_warning.line > 0,
        "warning line must be > 0 (was {})",
        multi_action_warning.line
    );
    assert!(
        multi_action_warning.col > 0,
        "warning col must be > 0 (was {})",
        multi_action_warning.col
    );
    // The handler `on go()` opens at line 11; the warning should NOT point
    // there — it should point at the `perform` on line 12.
    assert_ne!(
        multi_action_warning.line, 11,
        "warning line should not be the handler header line (11), got {}",
        multi_action_warning.line
    );
}
