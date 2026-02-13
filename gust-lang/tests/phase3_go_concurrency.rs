use gust_lang::{parse_program_with_errors, GoCodegen};

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
