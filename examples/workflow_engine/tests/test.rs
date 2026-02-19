// Integration tests for the WorkflowEngine state machine.
//
// Covers:
//   - Full pipeline runs all steps to Completed without any approval gate
//   - Pipeline pauses at an approval step, then resumes after approve
//   - Pipeline reaches an approval gate and is rejected -> Failed
//   - Approval with remaining > 1 transitions to Running, not Completed
//   - Invalid transitions return the correct error type

include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/workflow.g.rs"));

// LinearEffects models a two-step pipeline with no approval gates.
// Steps: "step-a" -> "step-b" (terminal).
struct LinearEffects;

impl WorkflowEngineEffects for LinearEffects {
    fn execute_step(&self, step_name: &String) -> String {
        format!("done:{}", step_name)
    }
    fn needs_approval(&self, _step_name: &String) -> bool {
        false
    }
    fn next_step_name(&self, current_step: &String) -> String {
        match current_step.as_str() {
            "step-a" => "step-b".to_string(),
            other => format!("after-{}", other),
        }
    }
}

// GatedEffects models a two-step pipeline where "step-b" requires approval.
// Steps: "step-a" -> "step-b" (requires approval, terminal).
struct GatedEffects;

impl WorkflowEngineEffects for GatedEffects {
    fn execute_step(&self, step_name: &String) -> String {
        format!("done:{}", step_name)
    }
    fn needs_approval(&self, step_name: &String) -> bool {
        step_name.as_str() == "step-b"
    }
    fn next_step_name(&self, current_step: &String) -> String {
        match current_step.as_str() {
            "step-a" => "step-b".to_string(),
            other => format!("after-{}", other),
        }
    }
}

// ThreeStepGatedEffects: "step-1" -> "step-2" (gated) -> "step-3" (terminal).
struct ThreeStepGatedEffects;

impl WorkflowEngineEffects for ThreeStepGatedEffects {
    fn execute_step(&self, step_name: &String) -> String {
        format!("done:{}", step_name)
    }
    fn needs_approval(&self, step_name: &String) -> bool {
        step_name.as_str() == "step-2"
    }
    fn next_step_name(&self, current_step: &String) -> String {
        match current_step.as_str() {
            "step-1" => "step-2".to_string(),
            "step-2" => "step-3".to_string(),
            other => format!("after-{}", other),
        }
    }
}

fn make_config(name: &str, total_steps: i64) -> WorkflowConfig {
    WorkflowConfig {
        name: name.to_string(),
        total_steps,
    }
}

// --- Completion path ---

#[test]
fn two_step_pipeline_completes_without_approval() {
    let effects = LinearEffects;
    let mut machine = WorkflowEngine::new(make_config("linear", 2));

    // Created state on construction.
    assert!(matches!(machine.state(), WorkflowEngineState::Created { .. }));

    // Created -> Running("step-a", 2)
    machine
        .start("step-a".to_string())
        .expect("start from Created should succeed");
    assert!(matches!(
        machine.state(),
        WorkflowEngineState::Running { current_step, remaining }
        if current_step == "step-a" && *remaining == 2
    ));

    // Running("step-a", 2) -> Running("step-b", 1)
    // next_remaining=1 > 0, next_step="step-b", needs_approval=false
    machine.advance(&effects).expect("advance step-a ok");
    assert!(matches!(
        machine.state(),
        WorkflowEngineState::Running { current_step, remaining }
        if current_step == "step-b" && *remaining == 1
    ));

    // Running("step-b", 1) -> Completed(1)
    // next_remaining=0, goto Completed
    machine.advance(&effects).expect("advance step-b ok");
    assert!(matches!(
        machine.state(),
        WorkflowEngineState::Completed { total_steps }
        if *total_steps == 1
    ));
}

// --- Approval path ---

#[test]
fn pipeline_pauses_at_approval_gate_then_completes() {
    let effects = GatedEffects;
    let mut machine = WorkflowEngine::new(make_config("gated", 2));

    machine.start("step-a".to_string()).expect("start ok");

    // Running("step-a", 2) -> AwaitingApproval("step-b", 1)
    // next_step="step-b", needs_approval("step-b")=true
    machine.advance(&effects).expect("advance step-a ok");
    assert!(matches!(
        machine.state(),
        WorkflowEngineState::AwaitingApproval { current_step, remaining }
        if current_step == "step-b" && *remaining == 1
    ));

    // remaining==1, so approve -> Completed(1)
    machine
        .approve("step-b".to_string())
        .expect("approve from AwaitingApproval should succeed");
    assert!(matches!(
        machine.state(),
        WorkflowEngineState::Completed { total_steps }
        if *total_steps == 1
    ));
}

// --- Rejection path ---

#[test]
fn pipeline_rejected_at_approval_gate_transitions_to_failed() {
    let effects = GatedEffects;
    let mut machine = WorkflowEngine::new(make_config("gated-reject", 2));

    machine.start("step-a".to_string()).expect("start ok");
    machine.advance(&effects).expect("advance step-a ok"); // -> AwaitingApproval("step-b", 1)

    assert!(matches!(
        machine.state(),
        WorkflowEngineState::AwaitingApproval { .. }
    ));

    machine
        .reject("compliance check failed".to_string())
        .expect("reject from AwaitingApproval should succeed");

    assert!(matches!(
        machine.state(),
        WorkflowEngineState::Failed { step_name, reason }
        if step_name == "step-b" && reason == "compliance check failed"
    ));
}

// --- Approval mid-pipeline (remaining > 1) ---

#[test]
fn approval_mid_pipeline_transitions_to_running_not_completed() {
    let effects = ThreeStepGatedEffects;
    let mut machine = WorkflowEngine::new(make_config("three-step", 3));

    machine.start("step-1".to_string()).expect("start ok");

    // Running("step-1", 3) -> AwaitingApproval("step-2", 2)
    // next_remaining=2 > 0, next_step="step-2", needs_approval=true
    machine.advance(&effects).expect("advance step-1 ok");
    assert!(matches!(
        machine.state(),
        WorkflowEngineState::AwaitingApproval { current_step, remaining }
        if current_step == "step-2" && *remaining == 2
    ));

    // remaining=2 > 1, so approve -> Running("step-3", 1)
    machine
        .approve("step-3".to_string())
        .expect("approve ok");
    assert!(matches!(
        machine.state(),
        WorkflowEngineState::Running { current_step, remaining }
        if current_step == "step-3" && *remaining == 1
    ));

    // Running("step-3", 1) -> Completed(1)
    machine.advance(&effects).expect("advance step-3 ok");
    assert!(matches!(
        machine.state(),
        WorkflowEngineState::Completed { .. }
    ));
}

// --- Invalid transitions ---

#[test]
fn start_from_running_returns_invalid_transition() {
    let mut machine = WorkflowEngine::new(make_config("invalid", 2));
    machine.start("step-a".to_string()).expect("first start ok");

    let err = machine
        .start("step-b".to_string())
        .expect_err("start from Running must fail");

    assert!(
        matches!(err, WorkflowEngineError::InvalidTransition { .. }),
        "expected InvalidTransition, got: {:?}",
        err
    );
}

#[test]
fn advance_from_created_returns_invalid_transition() {
    let effects = LinearEffects;
    let mut machine = WorkflowEngine::new(make_config("invalid", 2));

    let err = machine
        .advance(&effects)
        .expect_err("advance from Created must fail");

    assert!(
        matches!(err, WorkflowEngineError::InvalidTransition { .. }),
        "expected InvalidTransition, got: {:?}",
        err
    );
}

#[test]
fn reject_from_running_returns_invalid_transition() {
    let mut machine = WorkflowEngine::new(make_config("invalid", 2));
    machine.start("step-a".to_string()).expect("start ok");

    let err = machine
        .reject("should fail".to_string())
        .expect_err("reject from Running must fail");

    assert!(
        matches!(err, WorkflowEngineError::InvalidTransition { .. }),
        "expected InvalidTransition, got: {:?}",
        err
    );
}

#[test]
fn approve_from_running_returns_invalid_transition() {
    let mut machine = WorkflowEngine::new(make_config("invalid", 2));
    machine.start("step-a".to_string()).expect("start ok");

    let err = machine
        .approve("step-b".to_string())
        .expect_err("approve from Running must fail");

    assert!(
        matches!(err, WorkflowEngineError::InvalidTransition { .. }),
        "expected InvalidTransition, got: {:?}",
        err
    );
}

#[test]
fn advance_from_awaiting_approval_returns_invalid_transition() {
    let effects = GatedEffects;
    let mut machine = WorkflowEngine::new(make_config("invalid", 2));
    machine.start("step-a".to_string()).expect("start ok");
    machine.advance(&effects).expect("advance ok"); // -> AwaitingApproval

    let err = machine
        .advance(&effects)
        .expect_err("advance from AwaitingApproval must fail");

    assert!(
        matches!(err, WorkflowEngineError::InvalidTransition { .. }),
        "expected InvalidTransition, got: {:?}",
        err
    );
}
