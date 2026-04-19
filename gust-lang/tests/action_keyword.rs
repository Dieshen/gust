//! Tests for the `action` keyword (#40 PR 2).
//!
//! `action` is the non-idempotent / externally visible counterpart to
//! `effect`. Both share the same syntactic shape and codegen lowering in
//! v0.1 — the distinction is preserved in the AST (`EffectKind`) and the
//! formatter so replay-aware runtimes can consume it.

use gust_lang::ast::EffectKind;
use gust_lang::{format_program, parse_program, validate_program};

const SIMPLE_ACTION_SOURCE: &str = r#"
machine Notifier {
    state Idle
    state Pending(text: String)
    state Sent(timestamp: String)

    transition send: Pending -> Sent

    action post_message(channel: String, text: String) -> String

    on send(ctx: Ctx) {
        let ts: String = perform post_message(ctx.text, ctx.text);
        goto Sent(ts);
    }
}
"#;

#[test]
fn parses_action_keyword() {
    let program = parse_program(SIMPLE_ACTION_SOURCE).expect("should parse");
    let machine = &program.machines[0];
    assert_eq!(machine.effects.len(), 1);
    assert_eq!(machine.effects[0].name, "post_message");
    assert_eq!(machine.effects[0].kind, EffectKind::Action);
}

#[test]
fn parses_effect_keyword_as_effect_kind() {
    let source = r#"
machine Fetcher {
    state Start
    state Done(v: String)

    transition run: Start -> Done

    effect fetch(url: String) -> String

    on run() {
        let v: String = perform fetch("/");
        goto Done(v);
    }
}
"#;
    let program = parse_program(source).expect("should parse");
    let machine = &program.machines[0];
    assert_eq!(machine.effects.len(), 1);
    assert_eq!(machine.effects[0].kind, EffectKind::Effect);
}

#[test]
fn parses_mixed_effect_and_action() {
    let source = r#"
machine Mixed {
    state Start
    state Done(v: String)

    transition go: Start -> Done

    effect compute() -> String
    action publish(v: String) -> String

    on go() {
        let a: String = perform compute();
        let b: String = perform publish(a);
        goto Done(b);
    }
}
"#;
    let program = parse_program(source).expect("should parse");
    let machine = &program.machines[0];
    assert_eq!(machine.effects.len(), 2);
    let by_name: std::collections::HashMap<&str, EffectKind> = machine
        .effects
        .iter()
        .map(|e| (e.name.as_str(), e.kind))
        .collect();
    assert_eq!(by_name["compute"], EffectKind::Effect);
    assert_eq!(by_name["publish"], EffectKind::Action);
}

#[test]
fn parses_async_action() {
    let source = r#"
machine AsyncActor {
    state Start
    state Done(v: String)

    transition go: Start -> Done

    async action remote_call(url: String) -> String

    async on go() {
        let v: String = perform remote_call("/");
        goto Done(v);
    }
}
"#;
    let program = parse_program(source).expect("should parse");
    let machine = &program.machines[0];
    assert_eq!(machine.effects[0].kind, EffectKind::Action);
    assert!(machine.effects[0].is_async);
}

#[test]
fn validation_accepts_action_same_as_effect() {
    let program = parse_program(SIMPLE_ACTION_SOURCE).expect("should parse");
    let report = validate_program(&program, "action.gu", SIMPLE_ACTION_SOURCE);
    assert!(
        report.is_ok(),
        "action should validate identically to effect: {:?}",
        report.errors
    );
}

#[test]
fn formatter_emits_action_keyword() {
    let program = parse_program(SIMPLE_ACTION_SOURCE).expect("should parse");
    let formatted = format_program(&program);
    assert!(
        formatted.contains("action post_message("),
        "formatter should emit `action`, got:\n{formatted}"
    );
    assert!(
        !formatted.contains("effect post_message("),
        "formatter should not downgrade `action` to `effect`, got:\n{formatted}"
    );
}

#[test]
fn formatter_roundtrip_preserves_kind() {
    let program = parse_program(SIMPLE_ACTION_SOURCE).expect("first parse");
    let formatted = format_program(&program);
    let reparsed = parse_program(&formatted).expect("reparse");
    assert_eq!(reparsed.machines[0].effects[0].kind, EffectKind::Action);
}

#[test]
fn effect_kind_keyword_method() {
    assert_eq!(EffectKind::Effect.keyword(), "effect");
    assert_eq!(EffectKind::Action.keyword(), "action");
}

#[test]
fn existing_effect_programs_still_parse_and_default_to_effect_kind() {
    let source = r#"
machine Legacy {
    state Start
    state Done(v: String)

    transition go: Start -> Done

    effect legacy_effect() -> String

    on go() {
        let v: String = perform legacy_effect();
        goto Done(v);
    }
}
"#;
    let program = parse_program(source).expect("should parse");
    assert_eq!(
        program.machines[0].effects[0].kind,
        EffectKind::Effect,
        "pre-existing `effect` declarations must parse as EffectKind::Effect"
    );
}

#[test]
fn async_effect_and_async_action_both_parse() {
    let source = r#"
machine Both {
    state Start
    state Done(v: String)

    transition go: Start -> Done

    async effect a() -> String
    async action b(v: String) -> String

    async on go() {
        let x: String = perform a();
        let y: String = perform b(x);
        goto Done(y);
    }
}
"#;
    let program = parse_program(source).expect("should parse");
    let effects = &program.machines[0].effects;
    assert_eq!(effects.len(), 2);
    for e in effects {
        assert!(e.is_async, "{} should be async", e.name);
    }
    let by_name: std::collections::HashMap<&str, EffectKind> =
        effects.iter().map(|e| (e.name.as_str(), e.kind)).collect();
    assert_eq!(by_name["a"], EffectKind::Effect);
    assert_eq!(by_name["b"], EffectKind::Action);
}
