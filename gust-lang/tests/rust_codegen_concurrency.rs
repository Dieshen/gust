use gust_lang::{RustCodegen, parse_program_with_errors};

#[test]
fn parses_channel_annotations_send_spawn_and_timeout() {
    let source = r#"
channel OrderEvents: String (capacity: 32, mode: broadcast)

machine Parent(sends OrderEvents, supervises Worker(one_for_one)) {
    state Idle
    state Done

    transition run: Idle -> Done timeout 5s

    async on run() {
        send OrderEvents("started");
        spawn Worker();
        goto Done();
    }
}
"#;

    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    assert_eq!(program.channels.len(), 1);
    assert_eq!(program.machines.len(), 1);
    assert_eq!(program.machines[0].sends, vec!["OrderEvents"]);
    assert_eq!(program.machines[0].supervises.len(), 1);
    assert!(program.machines[0].transitions[0].timeout.is_some());
}

#[test]
fn rust_codegen_emits_channel_and_supervisor_hooks() {
    let source = r#"
channel OrderEvents: String (capacity: 32, mode: broadcast)

machine Parent(sends OrderEvents, supervises Worker(one_for_one)) {
    state Idle
    state Done

    transition run: Idle -> Done timeout 5s

    async on run() {
        send OrderEvents("started");
        spawn Worker();
        goto Done();
    }
}
"#;

    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    let generated = RustCodegen::new().generate(&program);
    assert!(generated.contains("pub struct OrderEventsChannel"));
    assert!(generated.contains("pub fn send_order_events("));
    assert!(generated.contains("supervisor: &gust_runtime::prelude::SupervisorRuntime"));
    assert!(generated.contains("tokio::time::Duration::from_secs(5)"));
    assert!(generated.contains("tokio::time::timeout("));
    assert!(generated.contains("transition 'run' timed out after"));
}

#[test]
fn parses_hour_timeout_unit() {
    let source = r#"
machine Worker {
    state Idle
    state Done

    transition run: Idle -> Done timeout 1h

    on run() {
        goto Done();
    }
}
"#;

    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    let generated = RustCodegen::new().generate(&program);
    assert!(generated.contains("tokio::time::Duration::from_secs(1 * 60 * 60)"));
}
