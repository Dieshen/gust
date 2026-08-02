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

/// The helper takes `&self`, which is only legal in an associated function, but
/// it was emitted at module scope — so every machine declaring `sends` produced
/// Rust that `rustc` rejected outright. The substring assertion above passed the
/// whole time, which is the point: presence is not placement.
///
/// The authoritative check is the `channel-annotations` fixture in
/// `codegen_backends.rs`, which compiles the output. This one just fails faster
/// and names the cause.
#[test]
fn channel_send_helper_is_an_inherent_method_not_a_free_function() {
    let source = r#"
channel OrderEvents: String (capacity: 32, mode: broadcast)

machine Parent(sends OrderEvents) {
    state Idle
    state Done

    transition run: Idle -> Done

    on run() {
        send OrderEvents("started");
        goto Done();
    }
}
"#;

    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    let generated = RustCodegen::new().generate(&program);

    let impl_block = generated
        .find("impl Parent {")
        .expect("machine should have an inherent impl block");
    let helper = generated
        .find("pub fn send_order_events(")
        .expect("a `sends` annotation should emit a send helper");

    assert!(
        helper > impl_block,
        "send helper must live inside the machine's impl block, not at module scope:\n{generated}"
    );
}

/// A channel struct's `new()` needs a matching `Default` or every consumer
/// building with `-D warnings` fails on `clippy::new_without_default`, in a file
/// they are told never to edit. See #110.
#[test]
fn channel_structs_get_a_default_impl() {
    let source = r#"
channel Broadcasts: String (capacity: 8, mode: broadcast)
channel Queue: String (capacity: 8, mode: mpsc)

machine Noop {
    state Idle
    state Done

    transition run: Idle -> Done

    on run() {
        goto Done();
    }
}
"#;

    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    let generated = RustCodegen::new().generate(&program);

    assert!(generated.contains("impl Default for BroadcastsChannel {"));
    assert!(generated.contains("impl Default for QueueChannel {"));
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
