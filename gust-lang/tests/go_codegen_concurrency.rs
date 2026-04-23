use gust_lang::{GoCodegen, parse_program, parse_program_with_errors};

#[test]
fn go_codegen_emits_channels_send_spawn_supervision_and_timeout_hooks() {
    let source = r#"
channel OrderEvents: String (capacity: 16, mode: broadcast)
channel WorkQueue: i64 (capacity: 64, mode: mpsc)

machine Worker {
    state Idle
    state Done
    transition run: Idle -> Done
    on run() {
        goto Done();
    }
}

machine Parent(sends OrderEvents, sends WorkQueue, supervises Worker(one_for_one)) {
    state Idle
    state Done

    transition run: Idle -> Done timeout 5s

    on run() {
        send OrderEvents("started");
        send WorkQueue(7);
        spawn Worker();
        goto Done();
    }
}
"#;

    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    let generated = GoCodegen::new().generate(&program, "testpkg");

    assert!(generated.contains("type OrderEventsChannel struct"));
    assert!(generated.contains("type WorkQueueChannel struct"));
    assert!(generated.contains("type SupervisorRuntime interface"));
    assert!(generated.contains("var ParentSupervision = []SupervisionSpec{"));
    assert!(generated.contains("SendOrderEvents("));
    assert!(generated.contains("SendWorkQueue("));
    assert!(generated.contains("order_eventsCh *OrderEventsChannel"));
    assert!(generated.contains("work_queueCh *WorkQueueChannel"));
    assert!(generated.contains("supervisor SupervisorRuntime"));
    assert!(generated.contains("order_eventsCh.Publish(\"started\")"));
    assert!(generated.contains("work_queueCh."));
    assert!(generated.contains("supervisor.SpawnNamed(\"Worker\""));
    assert!(generated.contains("context.WithTimeout(ctx, time.Duration(5) * time.Second)"));
}

#[test]
fn go_codegen_emits_hour_timeout_duration() {
    let source = r#"
machine Example {
    state Idle
    state Done
    transition run: Idle -> Done timeout 1h
    on run() {
        goto Done();
    }
}
"#;

    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    let generated = GoCodegen::new().generate(&program, "testpkg");
    assert!(generated.contains("time.Duration(1) * time.Hour"));
}

#[test]
fn test_go_ctx_field_rewrite() {
    let source = r#"
type Order { id: String }
type Money { cents: i64 }
machine Proc {
    state Pending(order: Order)
    state Done(total: Money)
    transition process: Pending -> Done
    effect calc(order: Order) -> Money
    on process(ctx: ProcCtx) {
        let total = perform calc(ctx.order);
        goto Done(total);
    }
}
"#;
    let program = parse_program(source).expect("should parse");
    let generated = GoCodegen::new().generate(&program, "main");

    assert!(!generated.contains("ctx."), "ctx.field should be rewritten");
    assert!(
        !generated.contains("ctx ProcCtx"),
        "ctx param should not be in Go method sig"
    );
    assert!(
        generated.contains("m.PendingData.Order"),
        "should access state data via m.XData.Y"
    );
}

#[test]
fn test_go_effects_interface_has_context() {
    let source = r#"
type Order { id: String }
machine Proc {
    state Idle
    state Done
    transition run: Idle -> Done
    async effect do_work(order: Order) -> String
    async on run() {
        let result = perform do_work(order);
        goto Done;
    }
}
"#;
    let program = parse_program(source).expect("should parse");
    let generated = GoCodegen::new().generate(&program, "main");

    // Bug 6: async effects should have context.Context param and error return
    assert!(
        generated.contains("DoWork(ctx context.Context,"),
        "async effects should take context.Context"
    );
    assert!(
        generated.contains(") (string, error)"),
        "async effects should return (T, error)"
    );
}

#[test]
fn test_go_clear_state_data_helper() {
    let source = r#"
type Data { value: i64 }
machine M {
    state A(x: Data)
    state B(y: Data)
    state C
    transition go_b: A -> B
    transition go_c: B -> C
}
"#;
    let program = parse_program(source).expect("should parse");
    let generated = GoCodegen::new().generate(&program, "main");

    // Bug 7: should use clearStateData() helper instead of individual nil assignments
    assert!(
        generated.contains("func (m *M) clearStateData()"),
        "should have clearStateData helper"
    );
    assert!(
        generated.contains("m.clearStateData()"),
        "goto should use clearStateData()"
    );
}

#[test]
fn test_go_async_effect_error_handling() {
    let source = r#"
type Order { id: String }
machine Proc {
    state Idle
    state Done
    transition run: Idle -> Done
    async effect fetch_data(order: Order) -> String
    effect log_msg(msg: String) -> bool
    async on run() {
        let data = perform fetch_data(order);
        perform fetch_data(order);
        perform log_msg("hello");
        goto Done;
    }
}
"#;
    let program = parse_program(source).expect("should parse");
    let generated = GoCodegen::new().generate(&program, "main");

    // Async let-perform must check error
    assert!(
        generated.contains("data, err := effects.FetchData(ctx,"),
        "async let should capture err"
    );
    assert!(
        generated.contains("if err != nil {"),
        "async let should check err"
    );

    // Bare async perform must also check error
    assert!(
        generated.contains("if _, err := effects.FetchData(ctx,"),
        "bare async perform should check err"
    );

    // Sync perform should NOT have error handling
    assert!(
        generated.contains("effects.LogMsg(\"hello\")"),
        "sync perform should be plain call"
    );
    assert!(
        !generated.contains("effects.LogMsg(\"hello\"); err"),
        "sync perform should not check err"
    );
}
