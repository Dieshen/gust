use gust_lang::ast::ChannelMode;
use gust_lang::{format_program, parse_program_with_errors, validate_program};

#[test]
fn parse_program_with_errors_suggests_keyword() {
    let source = r#"
machine Broken {
    state Start
    transision go: Start -> End
}
"#;

    let err = parse_program_with_errors(source, "test.gu").expect_err("expected parse error");
    let rendered = err.render(source);
    assert!(rendered.contains("unexpected identifier 'transision'"));
    assert!(rendered.contains("did you mean 'transition'?"));
}

#[test]
fn parse_reports_out_of_range_integer_literal_in_expression() {
    let source = r#"
machine BigInt {
    state Start
    transition go: Start -> Start
    on go() {
        let x = 999999999999999999999999999999;
        goto Start();
    }
}
"#;

    let err = parse_program_with_errors(source, "test.gu").expect_err("expected parse error");
    assert!(err.message.contains("out of range for i64"));
}

#[test]
fn parse_reports_out_of_range_integer_literal_in_timeout() {
    let source = r#"
machine BigTimeout {
    state Start
    transition go: Start -> Start timeout 999999999999999999999999999999s
    on go() {
        goto Start();
    }
}
"#;

    let err = parse_program_with_errors(source, "test.gu").expect_err("expected parse error");
    assert!(err.message.contains("out of range for i64"));
}

#[test]
fn parser_applies_channel_config_capacity_and_mode() {
    let source = r#"
channel jobs: String(capacity: 7, mode: mpsc)

machine Worker {
    state Idle
}
"#;

    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    assert_eq!(program.channels.len(), 1);
    assert_eq!(program.channels[0].capacity, Some(7));
    assert!(matches!(program.channels[0].mode, ChannelMode::Mpsc));
}

#[test]
fn validator_reports_undefined_target_and_unreachable_state() {
    let source = r#"
machine Test {
    state Start
    state Lonely
    transition go: Start -> Finish
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    let report = validate_program(&program, "test.gu", source);
    assert!(report
        .errors
        .iter()
        .any(|e| e.message.contains("undefined state 'Finish'")));
    assert!(report.errors.iter().any(|e| e.line > 1 && e.col > 1));
    assert!(report
        .warnings
        .iter()
        .any(|w| w.message.contains("unreachable state 'Lonely'")));
}

#[test]
fn validator_reports_undeclared_effect_and_bad_goto_arity() {
    let source = r#"
machine Test {
    state Start
    state Running(a: i64, b: i64)
    transition go: Start -> Running
    on go() {
        let x = perform missing_effect();
        goto Running(x);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    let report = validate_program(&program, "test.gu", source);
    assert!(report
        .errors
        .iter()
        .any(|e| e.message.contains("undeclared effect 'missing_effect'")));
    assert!(report.errors.iter().any(|e| e
        .message
        .contains("goto 'Running' expects 2 argument(s) but got 1")));
}

#[test]
fn validator_reports_undeclared_channel_and_machine_on_send_spawn() {
    let source = r#"
machine Test {
    state Start
    state End
    transition go: Start -> End
    on go() {
        send MissingChannel("msg");
        spawn MissingWorker();
        goto End();
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    let report = validate_program(&program, "test.gu", source);
    assert!(report
        .errors
        .iter()
        .any(|e| e.message.contains("undeclared channel 'MissingChannel'")));
    assert!(report
        .errors
        .iter()
        .any(|e| e.message.contains("undeclared machine 'MissingWorker'")));
}

#[test]
fn formatter_is_idempotent() {
    let source = r#"
machine Test {
    state Start
    state End
    transition go: Start -> End
    on go() {
        goto End();
    }
}
"#;
    let first_program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    let first = format_program(&first_program);
    let second_program =
        parse_program_with_errors(&first, "test.gu").expect("formatted source should parse");
    let second = format_program(&second_program);
    assert_eq!(first, second);
}

#[test]
fn test_formatter_preserves_handler_bodies() {
    let source = r#"
machine Door {
    state Locked(code: String)
    state Unlocked

    transition unlock: Locked -> Unlocked

    on unlock(attempt: String) {
        if attempt == code {
            goto Unlocked;
        }
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let formatted = format_program(&program);

    // Bug 2: formatter must NOT destroy handler bodies
    assert!(
        !formatted.contains("// formatter preserves structure only"),
        "handler body must be preserved"
    );
    assert!(
        formatted.contains("goto Unlocked"),
        "goto statement must survive formatting"
    );
    assert!(
        formatted.contains("if attempt == code"),
        "if statement must survive formatting"
    );
}

#[test]
fn validator_reports_ctx_field_not_in_from_state() {
    let source = r#"
machine Pipeline {
    state Running(name: String)
    state Failed(reason: String)
    state Recovered

    transition recover: Failed -> Recovered

    on recover() {
        perform log(ctx.name);
        goto Recovered;
    }

    effect log(msg: String) -> bool
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    // ctx.name is not a field of Failed (which only has `reason`)
    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("field 'name' not available in state 'Failed'")),
        "should report ctx.name not in Failed state, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.note.as_deref() == Some("available fields: reason")),
        "should list available fields"
    );
}

#[test]
fn validator_allows_valid_ctx_field_access() {
    let source = r#"
machine Pipeline {
    state Waiting(config: String)
    state Running(config: String)

    transition start: Waiting -> Running

    on start() {
        goto Running(ctx.config);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    // ctx.config is valid — config exists in Waiting state
    assert!(
        report.errors.is_empty(),
        "no errors expected for valid ctx.config, got: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_rejects_goto_to_undeclared_transition_target() {
    let source = r#"
machine Pipeline {
    state Pending
    state Validated
    state Failed

    transition validate: Pending -> Validated | Failed

    on validate() {
        goto Missing();
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("goto target 'Missing' is not a declared target of transition 'validate'")),
        "should reject goto to undeclared target, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.message.contains("valid targets are: Validated, Failed")),
        "should list valid targets"
    );
}

#[test]
fn validator_allows_goto_to_declared_transition_target() {
    let source = r#"
machine Pipeline {
    state Pending
    state Validated
    state Failed

    transition validate: Pending -> Validated | Failed

    on validate() {
        goto Validated();
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        !report
            .errors
            .iter()
            .any(|e| e.message.contains("goto target")),
        "should not reject valid goto target, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_rejects_goto_to_wrong_target_in_nested_blocks() {
    let source = r#"
machine Pipeline {
    state Pending
    state Validated
    state Failed

    transition validate: Pending -> Validated | Failed

    on validate() {
        if true {
            goto Pending();
        } else {
            goto Validated();
        }
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("goto target 'Pending' is not a declared target of transition 'validate'")),
        "should reject goto Pending in if-branch, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
    // goto Validated should be fine
    assert!(
        !report
            .errors
            .iter()
            .any(|e| e.message.contains("goto target 'Validated'")),
        "should allow goto Validated"
    );
}

#[test]
fn validator_rejects_handler_return_type() {
    let source = r#"
machine Counter {
    state Idle
    state Active

    transition start: Idle -> Active

    on start() -> i64 {
        goto Active();
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("handler return types are not yet supported")),
        "should reject handler with return type, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_allows_handler_without_return_type() {
    let source = r#"
machine Counter {
    state Idle
    state Active

    transition start: Idle -> Active

    on start() {
        goto Active();
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        !report
            .errors
            .iter()
            .any(|e| e.message.contains("return type")),
        "should not reject handler without return type, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_rejects_bare_return_in_handler() {
    let source = r#"
machine Demo {
    state Idle
    state Done

    transition go: Idle -> Done

    on go() {
        return 5;
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("return statements are not supported in handlers")),
        "should reject bare return in handler, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_rejects_nested_return_in_handler() {
    let source = r#"
machine Demo {
    state Idle
    state Done

    transition go: Idle -> Done

    on go() {
        if true {
            return 42;
        }
        goto Done();
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("return statements are not supported in handlers")),
        "should reject nested return in handler, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

// === Effect argument arity validation tests ===

#[test]
fn validator_allows_correct_effect_arity() {
    let source = r#"
machine Fetcher {
    state Start
    state Done(result: String)

    transition run: Start -> Done

    effect fetch_data(url: String, timeout: i64) -> String

    on run() {
        let result = perform fetch_data("example", 30);
        goto Done(result);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        !report
            .errors
            .iter()
            .any(|e| e.message.contains("effect 'fetch_data' expects")),
        "should not report arity error for correct args, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_reports_too_few_effect_args() {
    let source = r#"
machine Fetcher {
    state Start
    state Done(result: String)

    transition run: Start -> Done

    effect fetch_data(url: String, timeout: i64) -> String

    on run() {
        let result = perform fetch_data("example");
        goto Done(result);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("effect 'fetch_data' expects 2 argument(s) but got 1")),
        "should report too few args, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_reports_too_many_effect_args() {
    let source = r#"
machine Fetcher {
    state Start
    state Done(result: String)

    transition run: Start -> Done

    effect fetch_data(url: String) -> String

    on run() {
        let result = perform fetch_data("example", 30, true);
        goto Done(result);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("effect 'fetch_data' expects 1 argument(s) but got 3")),
        "should report too many args, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_reports_args_on_zero_param_effect() {
    let source = r#"
machine Pinger {
    state Start
    state Done(ok: bool)

    transition ping: Start -> Done

    effect ping_server() -> bool

    on ping() {
        let ok = perform ping_server("extra_arg");
        goto Done(ok);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("effect 'ping_server' expects 0 argument(s) but got 1")),
        "should report args on zero-param effect, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_checks_perform_as_expression_in_let() {
    let source = r#"
machine Worker {
    state Idle
    state Working(data: String)

    transition start: Idle -> Working

    effect load(key: String, ns: String) -> String

    on start() {
        let data = perform load("mykey");
        goto Working(data);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("effect 'load' expects 2 argument(s) but got 1")),
        "should check perform-as-expression arity, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_does_not_report_arity_for_unknown_effects() {
    let source = r#"
machine Worker {
    state Idle
    state Done

    transition go: Idle -> Done

    on go() {
        perform unknown_effect("a", "b", "c");
        goto Done();
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    // Should report "undeclared effect" but NOT an arity mismatch
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.message.contains("undeclared effect 'unknown_effect'")),
        "should report undeclared effect"
    );
    assert!(
        !report
            .errors
            .iter()
            .any(|e| e.message.contains("expects") && e.message.contains("argument(s) but got")),
        "should not report arity error for unknown effect, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}
