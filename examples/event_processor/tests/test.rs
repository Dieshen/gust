// Integration tests for the EventProcessor state machine.
//
// Covers:
//   - Happy path: valid event through the full pipeline to Completed
//   - Failure path: zero-priority event routed to Failed, then recovered via retry
//   - Invalid transition: calling a transition from the wrong state returns an error

include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/processor.g.rs"));

// TestEffects is a deterministic implementation used in tests.
struct TestEffects;

impl EventProcessorEffects for TestEffects {
    fn validate_event(&self, event: &Event) -> String {
        format!("ok:{}", event.source)
    }

    fn process_event(&self, event: &Event) -> ProcessedResult {
        ProcessedResult {
            event_id: format!("{}-test", event.source),
            output: format!("PROCESSED:{}", event.payload),
        }
    }
}

fn make_event(source: &str, payload: &str, priority: i64) -> Event {
    Event {
        source: source.to_string(),
        payload: payload.to_string(),
        priority,
    }
}

#[test]
fn happy_path_full_pipeline() {
    let effects = TestEffects;
    let mut machine = EventProcessor::new();

    // Machine starts at Idle.
    assert!(matches!(machine.state(), EventProcessorState::Idle));

    // Idle -> Receiving
    machine
        .receive(make_event("src-a", "data-payload", 5))
        .expect("receive from Idle should succeed");
    assert!(matches!(machine.state(), EventProcessorState::Receiving { .. }));

    // Receiving -> Validating (priority > 0 passes the branch)
    machine
        .validate(&effects)
        .expect("validate with positive priority should succeed");
    assert!(matches!(machine.state(), EventProcessorState::Validating { .. }));

    // Validating -> Completed (process calls process_event effect)
    machine
        .process(&effects)
        .expect("process should succeed");
    assert!(matches!(machine.state(), EventProcessorState::Completed { .. }));

    // Inspect the result fields.
    if let EventProcessorState::Completed { result } = machine.state() {
        assert_eq!(result.event_id, "src-a-test");
        assert_eq!(result.output, "PROCESSED:data-payload");
    } else {
        panic!("expected Completed state");
    }

    // Completed -> Idle via reset.
    machine.reset().expect("reset from Completed should succeed");
    assert!(matches!(machine.state(), EventProcessorState::Idle));
}

#[test]
fn low_priority_event_routes_to_failed() {
    let effects = TestEffects;
    let mut machine = EventProcessor::new();

    machine
        .receive(make_event("src-b", "ping", 0))
        .expect("receive should succeed");

    // Validate with priority == 0 triggers the else branch -> Failed.
    machine
        .validate(&effects)
        .expect("validate call returns Ok even when routing to Failed");

    // Machine must now be in Failed state.
    assert!(matches!(machine.state(), EventProcessorState::Failed { .. }));

    if let EventProcessorState::Failed { reason } = machine.state() {
        assert_eq!(reason, "event priority must be positive");
    } else {
        panic!("expected Failed state with specific reason");
    }
}

#[test]
fn retry_after_failure_restores_idle() {
    let effects = TestEffects;
    let mut machine = EventProcessor::new();

    machine
        .receive(make_event("src-c", "bad", -1))
        .expect("receive should succeed");
    machine.validate(&effects).expect("validate call ok");
    assert!(matches!(machine.state(), EventProcessorState::Failed { .. }));

    // Failed -> Idle via retry.
    machine.retry().expect("retry from Failed should succeed");
    assert!(matches!(machine.state(), EventProcessorState::Idle));
}

#[test]
fn invalid_transition_from_idle_returns_error() {
    let effects = TestEffects;
    let mut machine = EventProcessor::new();

    // validate requires Receiving state; calling it from Idle must fail.
    let err = machine
        .validate(&effects)
        .expect_err("validate from Idle must be an error");

    assert!(
        matches!(err, EventProcessorError::InvalidTransition { .. }),
        "expected InvalidTransition, got: {:?}",
        err
    );
}

#[test]
fn invalid_transition_process_from_receiving_returns_error() {
    let effects = TestEffects;
    let mut machine = EventProcessor::new();

    machine
        .receive(make_event("src-d", "data", 3))
        .expect("receive should succeed");

    // process requires Validating state; we are in Receiving, so it must fail.
    let err = machine
        .process(&effects)
        .expect_err("process from Receiving must be an error");

    assert!(
        matches!(err, EventProcessorError::InvalidTransition { .. }),
        "expected InvalidTransition, got: {:?}",
        err
    );
}

#[test]
fn negative_priority_also_routes_to_failed() {
    let effects = TestEffects;
    let mut machine = EventProcessor::new();

    machine
        .receive(make_event("src-e", "data", -5))
        .expect("receive should succeed");
    machine.validate(&effects).expect("validate call ok");

    assert!(matches!(machine.state(), EventProcessorState::Failed { .. }));
}

#[test]
fn full_cycle_receive_fail_retry_then_succeed() {
    let effects = TestEffects;
    let mut machine = EventProcessor::new();

    // First attempt: priority 0 -> Failed
    machine.receive(make_event("src-f", "payload", 0)).unwrap();
    machine.validate(&effects).unwrap();
    assert!(matches!(machine.state(), EventProcessorState::Failed { .. }));

    // Retry back to Idle
    machine.retry().unwrap();
    assert!(matches!(machine.state(), EventProcessorState::Idle));

    // Second attempt: priority > 0 -> Completed
    machine.receive(make_event("src-f", "payload", 1)).unwrap();
    machine.validate(&effects).unwrap();
    machine.process(&effects).unwrap();
    assert!(matches!(machine.state(), EventProcessorState::Completed { .. }));
}
