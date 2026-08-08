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
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.message.contains("undefined state 'Finish'"))
    );
    assert!(report.errors.iter().any(|e| e.line > 1 && e.col > 1));
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.message.contains("unreachable state 'Lonely'"))
    );
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
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.message.contains("undeclared effect 'missing_effect'"))
    );
    assert!(report.errors.iter().any(|e| {
        e.message
            .contains("goto 'Running' expects 2 argument(s) but got 1")
    }));
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
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.message.contains("undeclared channel 'MissingChannel'"))
    );
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.message.contains("undeclared machine 'MissingWorker'"))
    );
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

// === Effect return type checking (issue #30 item 2) ===

#[test]
fn validator_allows_matching_let_perform_annotation() {
    let source = r#"
machine Fetcher {
    state Start
    state Done(msg: String)

    transition run: Start -> Done

    effect load(key: String) -> String

    on run() {
        let s: String = perform load("x");
        goto Done(s);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        !report
            .errors
            .iter()
            .any(|e| e.message.contains("annotated as") && e.message.contains("returns")),
        "should not report mismatch when annotation matches effect return, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_rejects_mismatched_let_perform_annotation() {
    let source = r#"
machine Fetcher {
    state Start
    state Done(n: i64)

    transition run: Start -> Done

    effect load(key: String) -> String

    on run() {
        let n: i64 = perform load("x");
        goto Done(n);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("annotated as i64, but effect 'load' returns String")),
        "should report annotation/return mismatch, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_skips_perform_annotation_check_for_unknown_effect() {
    let source = r#"
machine Worker {
    state Idle
    state Done

    transition go: Idle -> Done

    on go() {
        let n: i64 = perform unknown_effect();
        goto Done();
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    // Unknown effect is reported separately; no annotation mismatch should fire.
    assert!(
        !report
            .errors
            .iter()
            .any(|e| e.message.contains("annotated as") && e.message.contains("returns")),
        "should not report annotation mismatch for unknown effect, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

// === If/else branch termination consistency (issue #30 item 3) ===

#[test]
fn validator_warns_when_one_branch_terminates_and_other_falls_through() {
    let source = r#"
machine Router {
    state Start(cond: bool)
    state Done

    transition check: Start -> Done

    on check(ctx) {
        if ctx.cond {
            goto Done();
        } else {
            let x: i64 = 1;
        }
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.message.contains("inconsistent if/else")
                && w.message.contains("may fall through")),
        "should warn on inconsistent if/else termination, got warnings: {:?}",
        report
            .warnings
            .iter()
            .map(|w| &w.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn validator_no_warning_when_both_branches_terminate() {
    let source = r#"
machine Router {
    state Start(cond: bool)
    state DoneA
    state DoneB

    transition check: Start -> DoneA | DoneB

    on check(ctx) {
        if ctx.cond {
            goto DoneA();
        } else {
            goto DoneB();
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
            .any(|w| w.message.contains("inconsistent if/else")),
        "should not warn when both branches terminate, got warnings: {:?}",
        report
            .warnings
            .iter()
            .map(|w| &w.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn validator_no_warning_when_neither_branch_terminates() {
    let source = r#"
machine Router {
    state Start(cond: bool)
    state Done

    transition check: Start -> Done

    on check(ctx) {
        if ctx.cond {
            let a: i64 = 1;
        } else {
            let b: i64 = 2;
        }
        goto Done();
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        !report
            .warnings
            .iter()
            .any(|w| w.message.contains("inconsistent if/else")),
        "should not warn when neither branch terminates, got warnings: {:?}",
        report
            .warnings
            .iter()
            .map(|w| &w.message)
            .collect::<Vec<_>>()
    );
}

// === Binary expression operand compatibility (issue #30 item 4) ===

#[test]
fn validator_allows_matching_binop_operands() {
    let source = r#"
machine Calc {
    state Start(a: i64)
    state Done(result: i64)

    transition go: Start -> Done

    on go(ctx) {
        let r: i64 = ctx.a + 1;
        goto Done(r);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        !report
            .errors
            .iter()
            .any(|e| e.message.contains("binary operator")),
        "should not report mismatch for matching operands, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_rejects_mismatched_binop_operands_int_vs_string() {
    let source = r#"
machine Calc {
    state Start
    state Done(result: i64)

    transition go: Start -> Done

    on go() {
        let r: i64 = 1 + "two";
        goto Done(r);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("binary operator '+' has incompatible operand types: i64 vs String")),
        "should report int + string mismatch, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_rejects_mismatched_comparison_operands() {
    let source = r#"
machine Router {
    state Start(cond: bool)
    state Done

    transition go: Start -> Done

    on go(ctx) {
        if 1 == "one" {
            goto Done();
        } else {
            goto Done();
        }
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    assert!(
        report.errors.iter().any(|e| e
            .message
            .contains("binary operator '==' has incompatible operand types")),
        "should report == operand mismatch, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn validator_skips_binop_check_when_operand_type_unknown() {
    let source = r#"
machine Calc {
    state Start
    state Done(result: i64)

    transition go: Start -> Done

    on go() {
        let r: i64 = unknown_fn() + 1;
        goto Done(r);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    // Can't infer unknown_fn()'s return; check should skip rather than false-positive.
    assert!(
        !report
            .errors
            .iter()
            .any(|e| e.message.contains("binary operator") && e.message.contains("incompatible")),
        "should skip binop check when operand type is unknown, got errors: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

// === Span coverage for #46 — previously these diagnostics reported line 0, col 0. ===

#[test]
fn binop_type_mismatch_diagnostic_points_to_source_location() {
    let source = r#"
machine Calc {
    state Start(a: i64, b: String)
    state Done

    transition go: Start -> Done

    on go(ctx) {
        let r: i64 = ctx.a + ctx.b;
        goto Done();
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    let binop_err = report
        .errors
        .iter()
        .find(|e| e.message.contains("binary operator") && e.message.contains("incompatible"))
        .expect("should report binop type mismatch");

    assert!(
        binop_err.line > 0,
        "binop diagnostic must carry a real line number (got {}), #46",
        binop_err.line
    );
    assert!(
        binop_err.col > 0,
        "binop diagnostic must carry a real column (got {}), #46",
        binop_err.col
    );
}

#[test]
fn if_branch_inconsistency_diagnostic_points_to_source_location() {
    let source = r#"
machine Router {
    state Start(cond: bool)
    state Done

    transition check: Start -> Done

    on check(ctx) {
        if ctx.cond {
            goto Done();
        } else {
            let x: i64 = 1;
        }
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    let if_warn = report
        .warnings
        .iter()
        .find(|w| w.message.contains("inconsistent if/else"))
        .expect("should warn on inconsistent if/else");

    assert!(
        if_warn.line > 0,
        "if/else diagnostic must carry a real line number (got {}), #46",
        if_warn.line
    );
    assert!(
        if_warn.col > 0,
        "if/else diagnostic must carry a real column (got {}), #46",
        if_warn.col
    );
}

// === Regression: Expr::Perform arity diagnostics carry real source spans ===

/// An arity mismatch on an inline `let x = perform effect(...)` must report
/// the `perform` call's own line and column, not `0:0`.
///
/// Before the fix `Expr::Perform` had no span field and `check_expr_perform_arity`
/// fell back to `Span::default()` (all zeroes).
#[test]
fn expr_perform_arity_diagnostic_points_to_source_location() {
    // The `perform load("only_one_arg")` call appears inside the handler body;
    // its line and column must be > 0 so IDEs can jump to the site.
    let source = r#"
machine SpanCheck {
    state Idle
    state Done(v: String)

    transition go: Idle -> Done

    effect load(key: String, ns: String) -> String

    on go() {
        let data: String = perform load("missing_second_arg");
        goto Done(data);
    }
}
"#;
    let program = parse_program_with_errors(source, "test.gu").expect("should parse");
    let report = validate_program(&program, "test.gu", source);

    let arity_error = report
        .errors
        .iter()
        .find(|e| {
            e.message
                .contains("effect 'load' expects 2 argument(s) but got 1")
        })
        .expect("expected arity error for perform-as-expression");

    assert!(
        arity_error.line > 0,
        "arity diagnostic for inline perform must have line > 0 (got {})",
        arity_error.line
    );
    assert!(
        arity_error.col > 0,
        "arity diagnostic for inline perform must have col > 0 (got {})",
        arity_error.col
    );
}

// ─── unused bindings and shadowed handler params (#100 root cause) ──────────

fn warnings_for(source: &str) -> Vec<String> {
    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    validate_program(&program, "test.gu", source)
        .warnings
        .into_iter()
        .map(|w| w.message)
        .collect()
}

/// An unused `let` only warns in Rust, but is a hard compile error in Go
/// (`declared and not used`), so the same source silently produces a Go package
/// that will not build. Reporting it against the `.gu` catches it once, at the
/// source. See #100.
#[test]
fn unused_let_binding_warns() {
    let warnings = warnings_for(
        r#"
machine Probe {
    state Idle(id: String)
    state Done(id: String)

    transition go: Idle -> Done

    effect check(a: String) -> bool

    on go(ctx) {
        let checked = perform check(ctx.id);
        goto Done(ctx.id);
    }
}
"#,
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("unused binding 'checked'")),
        "expected unused-binding warning, got: {warnings:?}"
    );
}

#[test]
fn used_let_binding_does_not_warn() {
    let warnings = warnings_for(
        r#"
machine Probe {
    state Idle(id: String)
    state Done(msg: String)

    transition go: Idle -> Done

    effect check(a: String) -> String

    on go(ctx) {
        let checked = perform check(ctx.id);
        goto Done(checked);
    }
}
"#,
    );
    assert!(
        !warnings.iter().any(|w| w.contains("unused binding")),
        "a binding that is read must not warn, got: {warnings:?}"
    );
}

/// There is deliberately no underscore-prefix exemption.
///
/// Gust never documented one — nothing in the grammar, docs, or the vault — and
/// bare `perform f();` has been valid since the first commit, so there is one
/// clear way to discard a result. Exempting `_name` would also be actively
/// misleading: Go accepts only a bare `_`, never `_name`, so an exempted
/// binding would sail past the very diagnostic meant to protect that backend.
/// See #100.
#[test]
fn underscore_prefixed_binding_still_warns() {
    let warnings = warnings_for(
        r#"
machine Probe {
    state Idle(id: String)
    state Done(id: String)

    transition go: Idle -> Done

    effect check(a: String) -> bool

    on go(ctx) {
        let _checked = perform check(ctx.id);
        goto Done(ctx.id);
    }
}
"#,
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("unused binding '_checked'")),
        "an underscore prefix must not suppress the warning, got: {warnings:?}"
    );
}

/// Bindings nested inside `if` / `match` must be reached too — the traversal
/// arms are exactly what cargo-mutants showed to be untested elsewhere in this
/// file.
#[test]
fn unused_binding_inside_nested_block_warns() {
    let warnings = warnings_for(
        r#"
machine Probe {
    state Idle(n: i64)
    state Done(n: i64)

    transition go: Idle -> Done

    effect check(a: i64) -> bool

    on go(ctx) {
        if ctx.n > 0 {
            let nested = perform check(ctx.n);
            goto Done(ctx.n);
        } else {
            goto Done(ctx.n);
        }
    }
}
"#,
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("unused binding 'nested'")),
        "must recurse into if/else blocks, got: {warnings:?}"
    );
}

/// Codegen destructures the from-state inside the transition method, so a
/// same-named handler parameter is shadowed and the emitted method argument is
/// dead.
#[test]
fn handler_param_shadowed_by_state_field_warns() {
    let warnings = warnings_for(
        r#"
type Request { id: String }

machine Router {
    state Idle(req: Request)
    state Done(req: Request)

    transition route: Idle -> Done

    on route(req: Request) {
        goto Done(req);
    }
}
"#,
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("handler parameter 'req' is shadowed")),
        "expected shadowed-param warning, got: {warnings:?}"
    );
}

#[test]
fn distinct_handler_param_name_does_not_warn() {
    let warnings = warnings_for(
        r#"
type Request { id: String }

machine Router {
    state Idle(req: Request)
    state Done(req: Request)

    transition route: Idle -> Done

    on route(incoming: Request) {
        goto Done(incoming);
    }
}
"#,
    );
    assert!(
        !warnings.iter().any(|w| w.contains("is shadowed")),
        "a distinctly-named parameter must not warn, got: {warnings:?}"
    );
}

// ─── validator traversal into nested blocks ────────────────────────────────
//
// cargo-mutants showed that deleting the `Statement::If` / `Statement::Match`
// recursion arms from these validators failed NO test. The behaviour was
// already correct — nested errors are reported — but nothing asserted it, so a
// refactor that stopped recursing would have passed the whole suite.
//
// Each test below nests the offending construct one level deep so it can only
// pass if the corresponding traversal arm survives.

fn errors_for(source: &str) -> Vec<String> {
    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    validate_program(&program, "test.gu", source)
        .errors
        .into_iter()
        .map(|e| e.message)
        .collect()
}

#[test]
fn perform_arity_is_checked_inside_if_and_match() {
    let inside_if = errors_for(
        r#"
machine M {
    state Idle(n: i64)
    state Done(n: i64)
    transition go: Idle -> Done
    effect one(a: i64) -> bool
    on go(ctx) {
        if ctx.n > 0 {
            let bad = perform one(1, 2, 3);
            goto Done(ctx.n);
        } else {
            goto Done(ctx.n);
        }
    }
}
"#,
    );
    assert!(
        inside_if.iter().any(|e| e.contains("expects 1 argument")),
        "arity must be checked inside if, got: {inside_if:?}"
    );

    let inside_match = errors_for(
        r#"
enum Tag { A, B }
machine M {
    state Idle(t: Tag)
    state Done(t: Tag)
    transition go: Idle -> Done
    effect one(a: i64) -> bool
    on go(ctx) {
        match t {
            Tag::A => { let bad = perform one(1, 2, 3); goto Done(t); }
            _ => { goto Done(t); }
        }
    }
}
"#,
    );
    assert!(
        inside_match
            .iter()
            .any(|e| e.contains("expects 1 argument")),
        "arity must be checked inside match arms, got: {inside_match:?}"
    );
}

/// `check_expr_perform_arity` recurses through operators, call arguments, and
/// field access — a `perform` buried in any of those must still be checked.
#[test]
fn perform_arity_is_checked_inside_nested_expressions() {
    let errors = errors_for(
        r#"
machine M {
    state Idle(n: i64)
    state Done(n: i64)
    transition go: Idle -> Done
    effect one(a: i64) -> i64
    on go(ctx) {
        let combined = perform one(1, 2) + ctx.n;
        goto Done(combined);
    }
}
"#,
    );
    assert!(
        errors.iter().any(|e| e.contains("expects 1 argument")),
        "arity must be checked inside a binary operator, got: {errors:?}"
    );
}

#[test]
fn goto_targets_are_checked_inside_match() {
    let errors = errors_for(
        r#"
enum Tag { A, B }
machine M {
    state Idle(t: Tag)
    state Done(t: Tag)
    transition go: Idle -> Done
    on go(ctx) {
        match t {
            Tag::A => { goto Nowhere(t); }
            _ => { goto Done(t); }
        }
    }
}
"#,
    );
    assert!(
        errors.iter().any(|e| e.contains("Nowhere")),
        "goto targets must be checked inside match arms, got: {errors:?}"
    );
}

#[test]
fn return_is_rejected_inside_nested_blocks() {
    let errors = errors_for(
        r#"
machine M {
    state Idle(n: i64)
    state Done(n: i64)
    transition go: Idle -> Done
    on go(ctx) {
        if ctx.n > 0 {
            return ctx.n;
        } else {
            goto Done(ctx.n);
        }
    }
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("return statements are not supported")),
        "return must be rejected inside if, got: {errors:?}"
    );
}

#[test]
fn send_targets_are_checked_inside_nested_blocks() {
    let errors = errors_for(
        r#"
channel Known: String
machine M {
    state Idle(n: i64)
    state Done(n: i64)
    transition go: Idle -> Done
    on go(ctx) {
        if ctx.n > 0 {
            send Unknown("x");
            goto Done(ctx.n);
        } else {
            goto Done(ctx.n);
        }
    }
}
"#,
    );
    assert!(
        errors.iter().any(|e| e.contains("undeclared channel")),
        "send targets must be checked inside if, got: {errors:?}"
    );
}

/// The guard `!channels.contains(channel)` survived mutation to `true`, meaning
/// nothing asserted that a *valid* send passes without error.
#[test]
fn valid_send_target_produces_no_error() {
    let errors = errors_for(
        r#"
channel Known: String
machine M {
    state Idle(n: i64)
    state Done(n: i64)
    transition go: Idle -> Done
    on go(ctx) {
        send Known("x");
        goto Done(ctx.n);
    }
}
"#,
    );
    assert!(
        !errors.iter().any(|e| e.contains("undeclared channel")),
        "a declared channel must not error, got: {errors:?}"
    );
}

#[test]
fn spawn_targets_are_checked_inside_nested_blocks() {
    let errors = errors_for(
        r#"
machine Child {
    state A
    state B
    transition t: A -> B
}
machine M {
    state Idle(n: i64)
    state Done(n: i64)
    transition go: Idle -> Done
    on go(ctx) {
        if ctx.n > 0 {
            spawn NoSuchMachine();
            goto Done(ctx.n);
        } else {
            goto Done(ctx.n);
        }
    }
}
"#,
    );
    assert!(
        errors.iter().any(|e| e.contains("NoSuchMachine")),
        "spawn targets must be checked inside if, got: {errors:?}"
    );
}

/// Mirror of the send guard: `!machines.contains(machine)` mutated to `true`
/// survived, so nothing asserted a valid spawn stays clean.
#[test]
fn valid_spawn_target_produces_no_error() {
    let errors = errors_for(
        r#"
machine Child {
    state A
    state B
    transition t: A -> B
}
machine M {
    state Idle(n: i64)
    state Done(n: i64)
    transition go: Idle -> Done
    on go(ctx) {
        spawn Child();
        goto Done(ctx.n);
    }
}
"#,
    );
    assert!(
        !errors
            .iter()
            .any(|e| e.contains("undeclared machine") || e.contains("Child")),
        "a declared machine must not error, got: {errors:?}"
    );
}

// The four survivors left after the first round of nested-traversal tests.
// cargo-mutants named them exactly: a bare `perform` statement, a perform
// under a unary operator, and `return` / `send` nested in `match` rather than
// `if`. Each is a distinct arm no existing test reached.

#[test]
fn bare_perform_statement_arity_is_checked() {
    let errors = errors_for(
        r#"
machine M {
    state Idle(n: i64)
    state Done(n: i64)
    transition go: Idle -> Done
    effect one(a: i64) -> ()
    on go(ctx) {
        perform one(1, 2, 3);
        goto Done(ctx.n);
    }
}
"#,
    );
    assert!(
        errors.iter().any(|e| e.contains("expects 1 argument")),
        "a bare perform statement must be arity-checked, got: {errors:?}"
    );
}

#[test]
fn perform_arity_is_checked_under_unary_operator() {
    let errors = errors_for(
        r#"
machine M {
    state Idle(n: i64)
    state Done(flag: bool)
    transition go: Idle -> Done
    effect truthy(a: i64) -> bool
    on go(ctx) {
        let flipped = !perform truthy(1, 2);
        goto Done(flipped);
    }
}
"#,
    );
    assert!(
        errors.iter().any(|e| e.contains("expects 1 argument")),
        "perform under a unary operator must be arity-checked, got: {errors:?}"
    );
}

#[test]
fn return_is_rejected_inside_match_arms() {
    let errors = errors_for(
        r#"
enum Tag { A, B }
machine M {
    state Idle(t: Tag)
    state Done(t: Tag)
    transition go: Idle -> Done
    on go(ctx) {
        match t {
            Tag::A => { return t; }
            _ => { goto Done(t); }
        }
    }
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("return statements are not supported")),
        "return must be rejected inside match arms, got: {errors:?}"
    );
}

// ---------------------------------------------------------------------------
// Result error-type erasure in the Go backend
// ---------------------------------------------------------------------------

/// Go signals failure with a single `error`, so `Result<T, E>` lowers to
/// `(T, error)` and `E` is lost. `String` survives via `error.Error()`; anything
/// else does not, and using the `Err` binding as an `E` will not compile. A
/// warning, not an error — the same source is valid Rust.
#[test]
fn non_string_result_error_type_warns_when_the_err_payload_is_used() {
    let warnings = warnings_for(
        r#"
machine Coded {
    state Start
    state Done(body: String)
    state Failed(code: i64)
    transition run: Start -> Done | Failed
    async effect fetch() -> Result<String, i64>
    async on run() {
        let outcome = perform fetch();
        match outcome {
            Ok(body) => { goto Done(body); }
            Err(code) => { goto Failed(code); }
        }
    }
}
"#,
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("Go cannot represent the error type of effect 'fetch'")),
        "got: {warnings:?}"
    );
}

#[test]
fn string_result_error_type_does_not_warn() {
    let warnings = warnings_for(
        r#"
machine Fetcher {
    state Start
    state Done(body: String)
    state Failed(reason: String)
    transition run: Start -> Done | Failed
    async effect fetch() -> Result<String, String>
    async on run() {
        let outcome = perform fetch();
        match outcome {
            Ok(body) => { goto Done(body); }
            Err(reason) => { goto Failed(reason); }
        }
    }
}
"#,
    );
    assert!(
        !warnings.iter().any(|w| w.contains("error type")),
        "a String error type round-trips through error.Error(), got: {warnings:?}"
    );
}

#[test]
fn discarded_err_payload_does_not_warn() {
    let warnings = warnings_for(
        r#"
machine Coded {
    state Start
    state Done(body: String)
    state Failed(code: i64)
    transition run: Start -> Done | Failed
    async effect fetch() -> Result<String, i64>
    async on run() {
        let outcome = perform fetch();
        match outcome {
            Ok(body) => { goto Done(body); }
            Err(_) => { goto Failed(0); }
        }
    }
}
"#,
    );
    assert!(
        !warnings.iter().any(|w| w.contains("error type")),
        "an unbound Err payload never reaches Go, got: {warnings:?}"
    );
}

#[test]
fn unmatched_non_string_result_does_not_warn() {
    let warnings = warnings_for(
        r#"
machine Coded {
    state Start
    state Done(code: i64)
    transition run: Start -> Done
    async effect fetch() -> Result<i64, i64>
    async on run() {
        let outcome = perform fetch();
        goto Done(outcome + 0);
    }
}
"#,
    );
    assert!(
        !warnings.iter().any(|w| w.contains("error type")),
        "without an Err arm the error is simply propagated, got: {warnings:?}"
    );
}

#[test]
fn send_targets_are_checked_inside_match_arms() {
    let errors = errors_for(
        r#"
channel Known: String
enum Tag { A, B }
machine M {
    state Idle(t: Tag)
    state Done(t: Tag)
    transition go: Idle -> Done
    on go(ctx) {
        match t {
            Tag::A => { send Unknown("x"); goto Done(t); }
            _ => { goto Done(t); }
        }
    }
}
"#,
    );
    assert!(
        errors.iter().any(|e| e.contains("undeclared channel")),
        "send targets must be checked inside match arms, got: {errors:?}"
    );
}

// ---------------------------------------------------------------------------
// `sends` / `receives` machine-header annotations (validate_channel_annotations)
//
// The annotation is a reference into the program-scope channel namespace, and
// `machine.sends` is what the Rust and Go backends iterate to emit the `send_*`
// helpers. Both resolve the name with `channels.iter().find(...)`, so a typo
// silently yields `None` and the helper vanishes from the generated API. That is
// wrong for every backend, and `send` to an undeclared channel is already a hard
// error, so this is an error too.
// ---------------------------------------------------------------------------

/// Full diagnostics (not just messages) so `note` and `help` can be asserted on.
fn channel_annotation_errors(source: &str) -> Vec<gust_lang::error::GustError> {
    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    validate_program(&program, "test.gu", source).errors
}

#[test]
fn sends_annotation_naming_an_undeclared_channel_is_an_error() {
    let errors = channel_annotation_errors(
        r#"
channel OrderEvents: String (capacity: 8, mode: broadcast)

machine Producer(sends OrderEvent) {
    state Idle
    state Done
    transition go: Idle -> Done
    on go() {
        goto Done();
    }
}
"#,
    );

    let err = errors
        .iter()
        .find(|e| e.message.contains("undeclared channel 'OrderEvent'"))
        .unwrap_or_else(|| panic!("a bogus 'sends' annotation must error, got: {errors:?}"));
    assert!(
        err.message
            .contains("'sends' annotation on machine 'Producer'"),
        "message must name the annotation kind and machine, got: {:?}",
        err.message
    );
    assert_eq!(
        err.note.as_deref(),
        Some(
            "a 'sends' annotation must name a channel declared at program scope; declared channels: OrderEvents"
        ),
        "note must state the rule and list the declared channels"
    );
    assert_eq!(
        err.help.as_deref(),
        Some("did you mean 'OrderEvents'?"),
        "help must carry the strsim did-you-mean suggestion"
    );
}

#[test]
fn receives_annotation_naming_an_undeclared_channel_is_an_error() {
    let errors = channel_annotation_errors(
        r#"
channel Notifications: String (capacity: 8, mode: mpsc)

machine Consumer(receives Notification) {
    state Idle
    state Done
    transition go: Idle -> Done
    on go() {
        goto Done();
    }
}
"#,
    );

    let err = errors
        .iter()
        .find(|e| e.message.contains("undeclared channel 'Notification'"))
        .unwrap_or_else(|| panic!("a bogus 'receives' annotation must error, got: {errors:?}"));
    assert!(
        err.message
            .contains("'receives' annotation on machine 'Consumer'"),
        "message must name the annotation kind and machine, got: {:?}",
        err.message
    );
    assert!(
        err.note
            .as_deref()
            .is_some_and(|n| n.starts_with("a 'receives' annotation must name a channel")),
        "note must be phrased for the 'receives' keyword, got: {:?}",
        err.note
    );
    assert_eq!(
        err.help.as_deref(),
        Some("did you mean 'Notifications'?"),
        "help must carry the strsim did-you-mean suggestion"
    );
}

#[test]
fn channel_annotation_error_is_anchored_to_the_machine_header() {
    let source = r#"channel Known: String

machine Producer(sends Unknown) {
    state Idle
    state Done
    transition go: Idle -> Done
    on go() {
        goto Done();
    }
}
"#;
    let errors = channel_annotation_errors(source);
    let err = errors
        .iter()
        .find(|e| e.message.contains("undeclared channel 'Unknown'"))
        .expect("expected a channel annotation error");
    // The annotation has no span of its own; the machine header is where it lives.
    assert_eq!(err.line, 3, "must point at the machine header line");
    assert!(
        err.render(source)
            .contains("machine Producer(sends Unknown)"),
        "the caret block must show the header line, got:\n{}",
        err.render(source)
    );
}

#[test]
fn channel_annotation_help_falls_back_when_nothing_is_close() {
    let errors = channel_annotation_errors(
        r#"
machine Producer(sends Telemetry) {
    state Idle
    state Done
    transition go: Idle -> Done
    on go() {
        goto Done();
    }
}
"#,
    );

    let err = errors
        .iter()
        .find(|e| e.message.contains("undeclared channel 'Telemetry'"))
        .unwrap_or_else(|| panic!("expected a channel annotation error, got: {errors:?}"));
    assert_eq!(
        err.note.as_deref(),
        Some(
            "a 'sends' annotation must name a channel declared at program scope; no channels are declared in this program"
        ),
        "note must say so when the program declares no channels at all"
    );
    assert_eq!(
        err.help.as_deref(),
        Some(
            "declare 'channel Telemetry: <Type>' at program scope, or remove 'sends Telemetry' from the machine header"
        ),
        "with no near match, help must still say what to do"
    );
}

#[test]
fn correctly_declared_channel_annotations_are_silent() {
    let errors = errors_for(
        r#"
channel OrderEvents: String (capacity: 8, mode: broadcast)
channel Notifications: String (capacity: 8, mode: mpsc)

machine Producer(sends OrderEvents, receives Notifications) {
    state Idle
    state Done
    transition go: Idle -> Done
    on go() {
        send OrderEvents("started");
        goto Done();
    }
}
"#,
    );
    assert!(
        errors.is_empty(),
        "annotations naming declared channels must not error, got: {errors:?}"
    );
}

#[test]
fn channel_annotated_but_never_sent_to_is_silent() {
    // `sends` alone is enough: it is what drives the generated `send_*` helper,
    // so a machine may declare the capability without exercising it in a handler.
    // There is no unused-channel diagnostic to double-report against.
    let errors = errors_for(
        r#"
channel OrderEvents: String (capacity: 8, mode: broadcast)

machine Producer(sends OrderEvents) {
    state Idle
    state Done
    transition go: Idle -> Done
    on go() {
        goto Done();
    }
}
"#,
    );
    assert!(
        errors.is_empty(),
        "an annotation with no matching send statement must not error, got: {errors:?}"
    );
}

#[test]
fn receives_annotation_with_no_consumer_code_is_silent() {
    // `receives` has no codegen consumer at all today beyond formatting, so it
    // must never be flagged merely for being unexercised.
    let errors = errors_for(
        r#"
channel Notifications: String (capacity: 8, mode: mpsc)

machine Consumer(receives Notifications) {
    state Idle
    state Done
    transition go: Idle -> Done
    on go() {
        goto Done();
    }
}
"#,
    );
    assert!(
        errors.is_empty(),
        "a 'receives' annotation must not require a consumer, got: {errors:?}"
    );
}

#[test]
fn channel_annotated_by_one_machine_and_sent_by_another_is_silent() {
    // Channels are program-scope, not machine-scope: the annotation on `Producer`
    // and the `send` in `Relay` both resolve against the same namespace.
    let errors = errors_for(
        r#"
channel OrderEvents: String (capacity: 8, mode: broadcast)

machine Producer(sends OrderEvents) {
    state Idle
    state Done
    transition go: Idle -> Done
    on go() {
        goto Done();
    }
}

machine Relay {
    state Idle
    state Done
    transition go: Idle -> Done
    on go() {
        send OrderEvents("relayed");
        goto Done();
    }
}
"#,
    );
    assert!(
        errors.is_empty(),
        "channels are program-scope; cross-machine use must not error, got: {errors:?}"
    );
}

#[test]
fn machine_with_no_channel_annotations_is_silent() {
    let errors = errors_for(
        r#"
channel OrderEvents: String (capacity: 8, mode: broadcast)

machine Plain {
    state Idle
    state Done
    transition go: Idle -> Done
    on go() {
        goto Done();
    }
}
"#,
    );
    assert!(
        errors.is_empty(),
        "a machine without annotations must not error, got: {errors:?}"
    );
}

#[test]
fn every_bad_channel_annotation_on_a_machine_is_reported() {
    let errors = errors_for(
        r#"
channel Good: String

machine Producer(sends Bad, sends Worse, receives Awful) {
    state Idle
    state Done
    transition go: Idle -> Done
    on go() {
        goto Done();
    }
}
"#,
    );
    for name in ["Bad", "Worse", "Awful"] {
        assert!(
            errors
                .iter()
                .any(|e| e.contains(&format!("undeclared channel '{name}'"))),
            "'{name}' must be reported; the loop must not stop at the first miss, got: {errors:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// spawn arity against the child's constructor
// ---------------------------------------------------------------------------

/// A machine is constructed from the fields of its **first** state, so that is
/// the arity a `spawn` argument list has to match.
///
/// Nothing checked this. `spawn Worker(job)` against a `Worker` whose first
/// state is fieldless generated `Worker::new(job)` against `fn new() -> Self`,
/// which `rustc` rejects with `E0061` — while `gust check` reported "Check
/// passed". It shipped in 0.4.0 because the supervision fixture happened to use
/// a child whose first state had exactly one field, so arity agreed by luck.
mod spawn_arity {
    use super::*;

    const FIELDLESS_CHILD: &str = r#"
machine Worker {
    state Idle
    state Busy(job: String)
    transition start: Idle -> Busy
    on start(job: String) { goto Busy(job); }
}
machine Boss(supervises Worker(one_for_one)) {
    state Ready(first: String)
    state Running(current: String)
    transition go: Ready -> Running
    on go(ctx) {
        spawn Worker(PLACEHOLDER);
        goto Running(ctx.first);
    }
}
"#;

    #[test]
    fn too_many_arguments_is_an_error() {
        let errors = errors_for(&FIELDLESS_CHILD.replace("PLACEHOLDER", "ctx.first"));
        assert!(
            errors
                .iter()
                .any(|e| e
                    .contains("spawn of 'Worker' passes 1 argument, but its constructor takes 0")),
            "got: {errors:?}"
        );
    }

    #[test]
    fn matching_zero_arguments_is_silent() {
        let errors = errors_for(&FIELDLESS_CHILD.replace("PLACEHOLDER", ""));
        assert!(errors.is_empty(), "expected no errors, got: {errors:?}");
    }

    #[test]
    fn too_few_arguments_is_an_error() {
        let source = r#"
machine Worker {
    state Idle(job: String)
    state Busy(job: String)
    transition start: Idle -> Busy
    on start(ctx) { goto Busy(ctx.job); }
}
machine Boss(supervises Worker(one_for_one)) {
    state Ready(first: String)
    state Running(current: String)
    transition go: Ready -> Running
    on go(ctx) {
        spawn Worker();
        goto Running(ctx.first);
    }
}
"#;
        let errors = errors_for(source);
        assert!(
            errors
                .iter()
                .any(|e| e
                    .contains("spawn of 'Worker' passes 0 arguments, but its constructor takes 1")),
            "got: {errors:?}"
        );
    }

    #[test]
    fn matching_one_argument_is_silent() {
        let source = r#"
machine Worker {
    state Idle(job: String)
    state Busy(job: String)
    transition start: Idle -> Busy
    on start(ctx) { goto Busy(ctx.job); }
}
machine Boss(supervises Worker(one_for_one)) {
    state Ready(first: String)
    state Running(current: String)
    transition go: Ready -> Running
    on go(ctx) {
        spawn Worker(ctx.first);
        goto Running(ctx.first);
    }
}
"#;
        assert!(errors_for(source).is_empty());
    }

    /// The check must reach a `spawn` nested inside control flow, like the
    /// undeclared-machine check beside it already does.
    #[test]
    fn arity_is_checked_inside_nested_blocks() {
        let source = r#"
machine Worker {
    state Idle
    state Busy(job: String)
    transition start: Idle -> Busy
    on start(job: String) { goto Busy(job); }
}
machine Boss(supervises Worker(one_for_one)) {
    state Ready(first: String)
    state Running(current: String)
    transition go: Ready -> Running
    on go(ctx) {
        if ctx.first == "now" {
            spawn Worker(ctx.first);
        }
        goto Running(ctx.first);
    }
}
"#;
        let errors = errors_for(source);
        assert!(
            errors.iter().any(|e| e.contains("spawn of 'Worker'")),
            "a spawn inside an if must still be checked, got: {errors:?}"
        );
    }
}
