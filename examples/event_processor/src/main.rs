include!("processor.g.rs");

// ProductionEffects implements the EventProcessorEffects trait with real logic.
// In production this would call external services; here it does deterministic transforms.
struct ProductionEffects;

impl EventProcessorEffects for ProductionEffects {
    fn validate_event(&self, event: &Event) -> String {
        // Return a validation token based on the event source and priority.
        // Priority check is enforced in the state machine handler, not here.
        format!("validated:{}:p{}", event.source, event.priority)
    }

    fn process_event(&self, event: &Event) -> ProcessedResult {
        // Transform the event payload into a processed result.
        // The event_id is derived from source + payload hash for traceability.
        let event_id = format!("{}-{}", event.source, event.payload.len());
        let output = format!("processed({}) -> {}", event.source, event.payload.to_uppercase());
        ProcessedResult { event_id, output }
    }
}

fn main() {
    let effects = ProductionEffects;

    println!("=== Event Processor Example ===\n");

    // --- Happy path: high-priority event through the full pipeline ---
    println!("-- Happy path: valid high-priority event --");
    let mut machine = EventProcessor::new();
    println!("Initial state: {:?}", machine.state());

    let event = Event {
        source: "sensor-1".to_string(),
        payload: "temperature:98.6".to_string(),
        priority: 10,
    };

    machine.receive(event).expect("receive should succeed from Idle");
    println!("After receive: {:?}", machine.state());

    machine.validate(&effects).expect("validate should succeed for priority > 0");
    println!("After validate: {:?}", machine.state());

    machine.process(&effects).expect("process should succeed from Validating");
    println!("After process: {:?}", machine.state());

    // Extract the result from the completed state.
    if let EventProcessorState::Completed { result } = machine.state() {
        println!("Result: event_id={}, output={}", result.event_id, result.output);
    }

    machine.reset().expect("reset should succeed from Completed");
    println!("After reset: {:?}\n", machine.state());

    // --- Failure path: zero-priority event gets rejected at validation ---
    println!("-- Failure path: zero-priority event rejected at validate --");
    let mut machine2 = EventProcessor::new();

    let bad_event = Event {
        source: "sensor-2".to_string(),
        payload: "heartbeat".to_string(),
        priority: 0, // zero priority triggers the failure branch
    };

    machine2.receive(bad_event).expect("receive should succeed");
    println!("After receive: {:?}", machine2.state());

    machine2.validate(&effects).expect("validate call itself succeeds (returns Ok even on Failed branch)");
    println!("After validate (bad priority): {:?}", machine2.state());

    // Confirm we are in the Failed state.
    assert!(
        matches!(machine2.state(), EventProcessorState::Failed { .. }),
        "expected Failed state after zero-priority event"
    );

    // Recover from failure and process a replacement event.
    machine2.retry().expect("retry should succeed from Failed");
    println!("After retry: {:?}", machine2.state());

    let recovery_event = Event {
        source: "sensor-2".to_string(),
        payload: "heartbeat".to_string(),
        priority: 1,
    };
    machine2.receive(recovery_event).expect("receive should succeed after retry");
    machine2.validate(&effects).expect("validate should succeed");
    machine2.process(&effects).expect("process should succeed");

    if let EventProcessorState::Completed { result } = machine2.state() {
        println!("Recovery result: event_id={}, output={}\n", result.event_id, result.output);
    }

    // --- Invalid transition: verify the machine rejects out-of-order calls ---
    println!("-- Invalid transition: attempt validate from Idle --");
    let mut machine3 = EventProcessor::new();
    let err = machine3
        .validate(&effects)
        .expect_err("validate from Idle must return an error");
    println!("Got expected error: {}\n", err);

    println!("All examples completed successfully.");
}
