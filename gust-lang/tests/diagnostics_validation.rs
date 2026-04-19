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

// === Match exhaustiveness tests ===

#[test]
fn validator_warns_on_non_exhaustive_enum_match() {
    let source = r#"
enum Status {
    Pending,
    Running,
    Done,
}

machine Tracker {
    state Idle(status: Status)
    state Finished(msg: String)

    transition finish: Idle -> Finished

    on finish() {
        match status {
            Status::Pending => { goto Finished("pending"); }
            Status::Done => { goto Finished("done"); }
        }
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report.warnings.iter().any(
            |w| w.message.contains("non-exhaustive match on enum 'Status'")
                && w.message.contains("Running")
        ),
        "should warn about missing variant 'Running', got warnings: {:?}",
        report
            .warnings
            .iter()
            .map(|w| &w.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn validator_no_warning_on_exhaustive_enum_match() {
    let source = r#"
enum Status {
    Pending,
    Done,
}

machine Tracker {
    state Idle(status: Status)
    state Finished(msg: String)

    transition finish: Idle -> Finished

    on finish() {
        match status {
            Status::Pending => { goto Finished("pending"); }
            Status::Done => { goto Finished("done"); }
        }
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        !report
            .warnings
            .iter()
            .any(|w| w.message.contains("non-exhaustive match")),
        "should not warn on exhaustive match, got warnings: {:?}",
        report
            .warnings
            .iter()
            .map(|w| &w.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn validator_no_warning_on_match_with_wildcard() {
    let source = r#"
enum Status {
    Pending,
    Running,
    Done,
}

machine Tracker {
    state Idle(status: Status)
    state Finished(msg: String)

    transition finish: Idle -> Finished

    on finish() {
        match status {
            Status::Done => { goto Finished("done"); }
            _ => { goto Finished("other"); }
        }
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        !report
            .warnings
            .iter()
            .any(|w| w.message.contains("non-exhaustive match")),
        "should not warn when wildcard arm is present, got warnings: {:?}",
        report
            .warnings
            .iter()
            .map(|w| &w.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn validator_exhaustive_enum_match_terminates_handler() {
    // An exhaustive enum match where every arm has a goto should not produce
    // the "code paths that don't end with a goto" warning.
    let source = r#"
enum Action {
    Start,
    Stop,
}

machine Worker {
    state Idle(action: Action)
    state Running
    state Stopped

    transition decide: Idle -> Running | Stopped

    on decide() {
        match action {
            Action::Start => { goto Running; }
            Action::Stop => { goto Stopped; }
        }
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        !report
            .warnings
            .iter()
            .any(|w| w.message.contains("don't end with a goto")),
        "exhaustive enum match with gotos should count as terminating, got warnings: {:?}",
        report
            .warnings
            .iter()
            .map(|w| &w.message)
            .collect::<Vec<_>>()
    );
}
// === Goto field type validation tests ===

#[test]
fn validator_allows_goto_with_matching_types() {
    let source = r#"
type Order {
    id: String,
    items: Vec<String>,
}
machine Processor {
    state Pending(order: Order)
    state Running(order: Order, count: i64, label: String)

    transition start: Pending -> Running

    on start() {
        goto Running(ctx.order, 42, "started");
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        !report
            .errors
            .iter()
            .any(|e| e.message.contains("argument") && e.message.contains("has type")),
        "should not report type errors for matching types, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_rejects_goto_with_mismatched_string_vs_int() {
    let source = r#"
machine Counter {
    state Idle
    state Running(count: i64)

    transition start: Idle -> Running

    on start() {
        goto Running("not a number");
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("goto 'Running' argument 1 has type String, but field 'count' expects i64")),
        "should report type mismatch String vs i64, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_rejects_goto_with_mismatched_int_vs_string() {
    let source = r#"
machine Namer {
    state Idle
    state Named(name: String)

    transition name_it: Idle -> Named

    on name_it() {
        goto Named(42);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("goto 'Named' argument 1 has type i64, but field 'name' expects String")),
        "should report type mismatch i64 vs String, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_rejects_goto_with_mismatched_bool_vs_string() {
    let source = r#"
machine Demo {
    state Idle
    state Done(result: String)

    transition finish: Idle -> Done

    on finish() {
        goto Done(true);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("goto 'Done' argument 1 has type bool, but field 'result' expects String")),
        "should report type mismatch bool vs String, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_checks_perform_result_type_in_goto() {
    let source = r#"
type Money {
    cents: i64,
}
machine Processor {
    state Pending
    state Done(total: Money)

    transition process: Pending -> Done

    effect calculate() -> String

    on process() {
        let result = perform calculate();
        goto Done(result);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("goto 'Done' argument 1 has type String, but field 'total' expects Money")),
        "should detect type mismatch from perform result, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_allows_perform_result_with_correct_type() {
    let source = r#"
type Money {
    cents: i64,
}
machine Processor {
    state Pending
    state Done(total: Money)

    transition process: Pending -> Done

    effect calculate() -> Money

    on process() {
        let result = perform calculate();
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
            .any(|e| e.message.contains("argument") && e.message.contains("has type")),
        "should not report type errors when perform returns correct type, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_checks_handler_param_types_in_goto() {
    let source = r#"
machine Pipeline {
    state Idle
    state Running(count: i64)

    transition start: Idle -> Running

    on start(name: String) {
        goto Running(name);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("goto 'Running' argument 1 has type String, but field 'count' expects i64")),
        "should detect handler param type mismatch, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_checks_types_in_nested_if_blocks() {
    let source = r#"
machine Pipeline {
    state Pending
    state Done(count: i64)
    state Failed(reason: String)

    transition finish: Pending -> Done | Failed

    on finish() {
        if true {
            goto Done("wrong type");
        } else {
            goto Failed(42);
        }
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("goto 'Done' argument 1 has type String, but field 'count' expects i64")),
        "should detect type mismatch in if-branch, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("goto 'Failed' argument 1 has type i64, but field 'reason' expects String")),
        "should detect type mismatch in else-branch, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_skips_check_for_unknown_types() {
    // FnCall return type is unknown — validator should NOT emit false positive
    let source = r#"
machine Pipeline {
    state Idle
    state Done(result: String)

    transition finish: Idle -> Done

    on finish() {
        let x = some_function();
        goto Done(x);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    // The function call result type is unknown — no type error should be emitted
    assert!(
        !report
            .errors
            .iter()
            .any(|e| e.message.contains("argument") && e.message.contains("has type")),
        "should not report type errors for unknown expression types, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_checks_ctx_field_type_in_goto() {
    let source = r#"
type Order {
    id: String,
}
machine Processor {
    state Pending(order: Order)
    state Done(count: i64)

    transition finish: Pending -> Done

    on finish() {
        goto Done(ctx.order);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("goto 'Done' argument 1 has type Order, but field 'count' expects i64")),
        "should detect ctx.field type mismatch, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_checks_nested_field_access_type_in_goto() {
    let source = r#"
type Order {
    id: String,
    count: i64,
}
machine Processor {
    state Pending(order: Order)
    state Done(label: String)

    transition finish: Pending -> Done

    on finish() {
        goto Done(ctx.order.count);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("goto 'Done' argument 1 has type i64, but field 'label' expects String")),
        "should detect nested field access type mismatch, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_allows_correct_nested_field_access_in_goto() {
    let source = r#"
type Order {
    id: String,
    count: i64,
}
machine Processor {
    state Pending(order: Order)
    state Done(id: String)

    transition finish: Pending -> Done

    on finish() {
        goto Done(ctx.order.id);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        !report
            .errors
            .iter()
            .any(|e| e.message.contains("argument") && e.message.contains("has type")),
        "should not report type errors for correct nested field access, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_checks_let_binding_with_explicit_type() {
    let source = r#"
machine Pipeline {
    state Idle
    state Done(name: String)

    transition finish: Idle -> Done

    on finish() {
        let count: i64 = 42;
        goto Done(count);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("goto 'Done' argument 1 has type i64, but field 'name' expects String")),
        "should detect explicit-typed let binding mismatch, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_checks_enum_path_type_in_goto() {
    let source = r#"
enum Status {
    Pending,
    Done(String),
}
machine Tracker {
    state Idle
    state Active(count: i64)

    transition start: Idle -> Active

    on start() {
        goto Active(Status::Pending);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("goto 'Active' argument 1 has type Status, but field 'count' expects i64")),
        "should detect enum path type mismatch, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_allows_enum_path_with_correct_type() {
    let source = r#"
enum Status {
    Pending,
    Done(String),
}
machine Tracker {
    state Idle
    state Active(status: Status)

    transition start: Idle -> Active

    on start() {
        goto Active(Status::Pending);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        !report
            .errors
            .iter()
            .any(|e| e.message.contains("argument") && e.message.contains("has type")),
        "should not report type errors for correct enum path, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_checks_multiple_mismatched_args() {
    let source = r#"
machine Demo {
    state Idle
    state Done(name: String, count: i64, flag: bool)

    transition finish: Idle -> Done

    on finish() {
        goto Done(42, "wrong", "not bool");
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    let type_errors: Vec<&String> = report
        .errors
        .iter()
        .filter(|e| e.message.contains("argument") && e.message.contains("has type"))
        .map(|e| &e.message)
        .collect();

    assert!(
        type_errors.len() >= 3,
        "should report all three type mismatches, got {} type errors: {:?}",
        type_errors.len(),
        type_errors
    );
}

#[test]
fn validator_checks_comparison_op_produces_bool() {
    let source = r#"
machine Demo {
    state Idle
    state Done(name: String)

    transition finish: Idle -> Done

    on finish() {
        goto Done(1 > 2);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("goto 'Done' argument 1 has type bool, but field 'name' expects String")),
        "should detect comparison op produces bool, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_does_not_type_check_when_arity_mismatches() {
    // When arity already mismatches, don't also emit type errors
    let source = r#"
machine Demo {
    state Idle
    state Done(name: String, count: i64)

    transition finish: Idle -> Done

    on finish() {
        goto Done("only one arg");
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    // Should have arity error but no type error
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.message.contains("expects 2 argument(s) but got 1")),
        "should report arity error"
    );
    assert!(
        !report
            .errors
            .iter()
            .any(|e| e.message.contains("has type") && e.message.contains("but field")),
        "should not report type errors when arity mismatches, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}
