//! Tests that codegen preserves the `effect` vs `action` distinction (#40 PR 3).
//!
//! `action` currently lowers identically to `effect`, but the generated code
//! must carry a marker (comment / attribute) so downstream tooling and humans
//! can tell them apart without re-parsing the .gu source.

use gust_lang::{GoCodegen, RustCodegen, parse_program};

const MIXED_SRC: &str = r#"
machine Hybrid {
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

const ONLY_EFFECT_SRC: &str = r#"
machine Pure {
    state Start
    state Done(v: String)

    transition go: Start -> Done

    effect compute() -> String

    on go() {
        let a: String = perform compute();
        goto Done(a);
    }
}
"#;

#[test]
fn rust_codegen_marks_action_methods() {
    let program = parse_program(MIXED_SRC).expect("should parse");
    let code = RustCodegen::new().generate(&program);

    // The action method carries the marker comment.
    assert!(
        code.contains("/// gust:action -- not replay-safe / externally visible"),
        "Rust codegen should mark action methods with a machine-readable rustdoc comment. Got:\n{code}"
    );

    // The marker appears immediately before the `publish` method.
    let lines: Vec<&str> = code.lines().collect();
    let publish_idx = lines
        .iter()
        .position(|l| l.contains("fn publish"))
        .expect("should emit `fn publish`");
    assert!(publish_idx > 0, "publish should not be at line 0");
    assert!(
        lines[publish_idx - 1].contains("/// gust:action -- not replay-safe / externally visible"),
        "marker should be directly above fn publish, got:\n{}\n{}",
        lines[publish_idx - 1],
        lines[publish_idx]
    );
}

#[test]
fn rust_codegen_does_not_mark_plain_effect_methods() {
    let program = parse_program(ONLY_EFFECT_SRC).expect("should parse");
    let code = RustCodegen::new().generate(&program);
    assert!(
        code.contains("/// gust:effect -- replay-safe / idempotent"),
        "plain effects must be marked with effect annotations. Got:\n{code}"
    );
    assert!(
        !code.contains("/// gust:action"),
        "plain effects must not be marked as actions. Got:\n{code}"
    );
    assert!(code.contains("fn compute"));
}

#[test]
fn go_codegen_marks_action_methods() {
    let program = parse_program(MIXED_SRC).expect("should parse");
    let code = GoCodegen::new().generate(&program, "hybrid");

    assert!(
        code.contains("// gust:action -- not replay-safe / externally visible"),
        "Go codegen should mark action methods with a // comment. Got:\n{code}"
    );

    // Marker must appear in the interface block, directly before the Publish method.
    let lines: Vec<&str> = code.lines().collect();
    let publish_idx = lines
        .iter()
        .position(|l| l.contains("Publish("))
        .expect("Publish method should be generated");
    assert!(publish_idx > 0, "Publish should not be at line 0");
    assert!(
        lines[publish_idx - 1].contains("// gust:action -- not replay-safe / externally visible"),
        "marker should precede Publish, got:\n{}\n{}",
        lines[publish_idx - 1],
        lines[publish_idx]
    );
}

#[test]
fn go_codegen_does_not_mark_plain_effect_methods() {
    let program = parse_program(ONLY_EFFECT_SRC).expect("should parse");
    let code = GoCodegen::new().generate(&program, "pure");
    assert!(
        code.contains("// gust:effect -- replay-safe / idempotent"),
        "plain Go effects must be marked with effect annotations. Got:\n{code}"
    );
    assert!(
        !code.contains("// gust:action"),
        "plain effects must not be marked as actions. Got:\n{code}"
    );
}

#[test]
fn codegen_marker_count_matches_action_count() {
    let program = parse_program(MIXED_SRC).expect("should parse");
    let rust_code = RustCodegen::new().generate(&program);
    let go_code = GoCodegen::new().generate(&program, "hybrid");

    let rust_action_markers = rust_code.matches("/// gust:action").count();
    let rust_effect_markers = rust_code.matches("/// gust:effect").count();
    let go_action_markers = go_code.matches("// gust:action").count();
    let go_effect_markers = go_code.matches("// gust:effect").count();

    // One `effect compute` and one `action publish` → exactly one marker each.
    assert_eq!(rust_action_markers, 1, "Rust: expected 1 action marker");
    assert_eq!(rust_effect_markers, 1, "Rust: expected 1 effect marker");
    assert_eq!(go_action_markers, 1, "Go: expected 1 action marker");
    assert_eq!(go_effect_markers, 1, "Go: expected 1 effect marker");
}

#[test]
fn action_only_machine_still_generates_valid_trait() {
    let source = r#"
machine OnlyActions {
    state Start
    state Done(v: String)

    transition go: Start -> Done

    action publish(v: String) -> String

    on go() {
        let b: String = perform publish("hi");
        goto Done(b);
    }
}
"#;
    let program = parse_program(source).expect("should parse");
    let code = RustCodegen::new().generate(&program);
    assert!(code.contains("pub trait OnlyActionsEffects"));
    assert!(code.contains("fn publish"));
    assert!(code.contains("/// gust:action -- not replay-safe / externally visible"));
}

#[test]
fn rust_codegen_places_kind_annotations_above_every_trait_method() {
    let program = parse_program(MIXED_SRC).expect("should parse");
    let code = RustCodegen::new().generate(&program);
    let lines: Vec<&str> = code.lines().collect();

    let compute_idx = lines
        .iter()
        .position(|l| l.contains("fn compute"))
        .expect("should emit `fn compute`");
    let publish_idx = lines
        .iter()
        .position(|l| l.contains("fn publish"))
        .expect("should emit `fn publish`");

    assert_eq!(
        lines[compute_idx - 1].trim(),
        "/// gust:effect -- replay-safe / idempotent"
    );
    assert_eq!(
        lines[publish_idx - 1].trim(),
        "/// gust:action -- not replay-safe / externally visible"
    );
}

#[test]
fn go_codegen_places_kind_annotations_above_every_interface_method() {
    let program = parse_program(MIXED_SRC).expect("should parse");
    let code = GoCodegen::new().generate(&program, "hybrid");
    let lines: Vec<&str> = code.lines().collect();

    let compute_idx = lines
        .iter()
        .position(|l| l.contains("Compute("))
        .expect("should emit `Compute`");
    let publish_idx = lines
        .iter()
        .position(|l| l.contains("Publish("))
        .expect("should emit `Publish`");

    assert_eq!(
        lines[compute_idx - 1].trim(),
        "// gust:effect -- replay-safe / idempotent"
    );
    assert_eq!(
        lines[publish_idx - 1].trim(),
        "// gust:action -- not replay-safe / externally visible"
    );
}
