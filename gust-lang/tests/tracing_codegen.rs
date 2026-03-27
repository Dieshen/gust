use gust_lang::{parse_program_with_errors, RustCodegen};

fn simple_machine_source() -> &'static str {
    r#"
machine Door {
    state Closed
    state Open

    transition open: Closed -> Open
    transition close: Open -> Closed

    on open() {
        goto Open();
    }

    on close() {
        goto Closed();
    }
}
"#
}

fn machine_with_effects_source() -> &'static str {
    r#"
machine Processor {
    state Idle
    state Processing(item: String)
    state Done(result: String)

    transition start: Idle -> Processing
    transition finish: Processing -> Done

    effect validate(item: String) -> bool
    async effect process(item: String) -> String

    on start(item: String) {
        perform validate(item);
        goto Processing(item);
    }

    async on finish(ctx: FinishCtx) {
        let result = perform process(ctx.item);
        goto Done(result);
    }
}
"#
}

#[test]
fn tracing_disabled_by_default_no_tracing_output() {
    let program =
        parse_program_with_errors(simple_machine_source(), "test.gu").expect("should parse");
    let generated = RustCodegen::new().generate(&program);

    assert!(
        !generated.contains("tracing::"),
        "default codegen should not contain tracing macros"
    );
    assert!(
        !generated.contains("#[cfg(feature = \"tracing\")]"),
        "default codegen should not contain tracing cfg guards"
    );
}

#[test]
fn tracing_disabled_explicitly_no_tracing_output() {
    let program =
        parse_program_with_errors(simple_machine_source(), "test.gu").expect("should parse");
    let generated = RustCodegen::new().with_tracing(false).generate(&program);

    assert!(
        !generated.contains("tracing::"),
        "explicitly disabled tracing should not contain tracing macros"
    );
}

#[test]
fn tracing_enabled_emits_use_tracing_import() {
    let program =
        parse_program_with_errors(simple_machine_source(), "test.gu").expect("should parse");
    let generated = RustCodegen::new().with_tracing(true).generate(&program);

    assert!(
        generated.contains("#[cfg(feature = \"tracing\")]"),
        "tracing-enabled codegen should contain cfg guard"
    );
    assert!(
        generated.contains("use tracing;"),
        "tracing-enabled codegen should import tracing crate"
    );
}

#[test]
fn tracing_enabled_emits_transition_spans() {
    let program =
        parse_program_with_errors(simple_machine_source(), "test.gu").expect("should parse");
    let generated = RustCodegen::new().with_tracing(true).generate(&program);

    // Should have tracing spans for both transitions
    assert!(
        generated.contains("tracing::info_span!(\"open\""),
        "should emit info_span for open transition"
    );
    assert!(
        generated.contains("tracing::info_span!(\"close\""),
        "should emit info_span for close transition"
    );
}

#[test]
fn tracing_enabled_emits_transition_info_events() {
    let program =
        parse_program_with_errors(simple_machine_source(), "test.gu").expect("should parse");
    let generated = RustCodegen::new().with_tracing(true).generate(&program);

    // Should have tracing::info! events for state transitions
    assert!(
        generated.contains("tracing::info!(machine = \"Door\""),
        "should emit info event with machine name"
    );
    assert!(
        generated.contains("transition = \"open\""),
        "should emit info event with transition name"
    );
    assert!(
        generated.contains("from = \"Closed\""),
        "should emit info event with from state"
    );
    assert!(
        generated.contains("to = \"Open\""),
        "should emit info event with to state"
    );
    assert!(
        generated.contains("\"state transition\""),
        "should include 'state transition' message"
    );
}

#[test]
fn tracing_enabled_emits_effect_invocation_events() {
    let program =
        parse_program_with_errors(machine_with_effects_source(), "test.gu").expect("should parse");
    let generated = RustCodegen::new().with_tracing(true).generate(&program);

    // Should have tracing events for effect invocations
    assert!(
        generated.contains("effect = \"validate\""),
        "should emit info event for validate effect invocation"
    );
    assert!(
        generated.contains("effect = \"process\""),
        "should emit info event for process effect invocation"
    );
    assert!(
        generated.contains("\"effect invocation\""),
        "should include 'effect invocation' message"
    );
}

#[test]
fn tracing_cfg_guards_on_all_tracing_statements() {
    let program =
        parse_program_with_errors(simple_machine_source(), "test.gu").expect("should parse");
    let generated = RustCodegen::new().with_tracing(true).generate(&program);

    // Every tracing statement should be preceded by a cfg guard
    let lines: Vec<&str> = generated.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("tracing::") || trimmed.starts_with("let __tracing_") {
            // The preceding non-empty line should be a cfg guard
            assert!(
                i > 0 && lines[i - 1].trim() == "#[cfg(feature = \"tracing\")]",
                "tracing statement at line {} should be guarded by #[cfg(feature = \"tracing\")], but preceding line is: '{}'",
                i + 1,
                if i > 0 { lines[i - 1] } else { "<start of file>" }
            );
        }
    }
}

#[test]
fn tracing_enabled_does_not_affect_core_codegen() {
    let program =
        parse_program_with_errors(simple_machine_source(), "test.gu").expect("should parse");
    let without_tracing = RustCodegen::new().generate(&program);
    let with_tracing = RustCodegen::new().with_tracing(true).generate(&program);

    // Core structures should still be present
    assert!(with_tracing.contains("pub enum DoorState"));
    assert!(with_tracing.contains("pub struct Door"));
    assert!(with_tracing.contains("pub fn new()"));
    assert!(with_tracing.contains("fn open("));
    assert!(with_tracing.contains("fn close("));

    // The non-tracing code should not have any tracing artifacts
    assert!(!without_tracing.contains("__tracing_span"));
    assert!(!without_tracing.contains("__tracing_guard"));
}

#[test]
fn tracing_with_multi_target_transition() {
    let source = r#"
machine Validator {
    state Pending
    state Valid
    state Invalid(reason: String)

    transition validate: Pending -> Valid | Invalid

    on validate() {
        goto Valid();
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let generated = RustCodegen::new().with_tracing(true).generate(&program);

    // Multi-target transitions should list all targets
    assert!(
        generated.contains("to = \"Valid | Invalid\""),
        "multi-target transition should list all target states"
    );
}
