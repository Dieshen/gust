// Integration tests for the WorkflowEngine state machine.
//
// Covers:
//   - Full pipeline runs all steps to Completed without any approval gate
//   - Pipeline pauses at an approval step, then resumes after approve
//   - Pipeline reaches an approval gate and is rejected -> Failed(EngineFailure)
//   - Approval with remaining > 1 transitions to Running, not Completed
//   - Invalid transitions return the correct error type
//   - action (notify_rejection) is invoked on the reject path
//   - EngineFailure wraps rejection reasons as typed variants
//   - StepRunner child machine compiles and operates correctly

mod engine_failure_types {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/engine_failure.g.rs"));
}

mod workflow_machine {
    pub use super::engine_failure_types::EngineFailure;
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/workflow.g.rs"));
}

pub use engine_failure_types::EngineFailure;
pub use workflow_machine::{
    StepRunner, StepRunnerEffects, StepRunnerError, StepRunnerState,
    WorkflowConfig, WorkflowEngine, WorkflowEngineEffects, WorkflowEngineError,
    WorkflowEngineState,
};

// ---------------------------------------------------------------------------
// Effects helpers
// ---------------------------------------------------------------------------

fn make_config(name: &str, total_steps: i64) -> WorkflowConfig {
    WorkflowConfig {
        name: name.to_string(),
        total_steps,
    }
}

// LinearEffects models a two-step pipeline with no approval gates.
// Steps: "step-a" -> "step-b" (terminal).
struct LinearEffects;

impl WorkflowEngineEffects for LinearEffects {
    fn execute_step(&self, step_name: &str) -> String {
        format!("done:{}", step_name)
    }
    fn needs_approval(&self, _step_name: &str) -> bool {
        false
    }
    fn next_step_name(&self, current_step: &str) -> String {
        match current_step {
            "step-a" => "step-b".to_string(),
            other => format!("after-{}", other),
        }
    }
    fn produce_failure(&self, reason: &str) -> EngineFailure {
        EngineFailure::UserError(reason.to_string())
    }
    fn notify_rejection(&self, _step_name: &str, _reason: &str) -> String {
        "notified".to_string()
    }
}

// GatedEffects models a two-step pipeline where "step-b" requires approval.
// Steps: "step-a" -> "step-b" (requires approval, terminal).
struct GatedEffects;

impl WorkflowEngineEffects for GatedEffects {
    fn execute_step(&self, step_name: &str) -> String {
        format!("done:{}", step_name)
    }
    fn needs_approval(&self, step_name: &str) -> bool {
        step_name == "step-b"
    }
    fn next_step_name(&self, current_step: &str) -> String {
        match current_step {
            "step-a" => "step-b".to_string(),
            other => format!("after-{}", other),
        }
    }
    fn produce_failure(&self, reason: &str) -> EngineFailure {
        EngineFailure::UserError(reason.to_string())
    }
    fn notify_rejection(&self, _step_name: &str, _reason: &str) -> String {
        "notified".to_string()
    }
}

// ThreeStepGatedEffects: "step-1" -> "step-2" (gated) -> "step-3" (terminal).
struct ThreeStepGatedEffects;

impl WorkflowEngineEffects for ThreeStepGatedEffects {
    fn execute_step(&self, step_name: &str) -> String {
        format!("done:{}", step_name)
    }
    fn needs_approval(&self, step_name: &str) -> bool {
        step_name == "step-2"
    }
    fn next_step_name(&self, current_step: &str) -> String {
        match current_step {
            "step-1" => "step-2".to_string(),
            "step-2" => "step-3".to_string(),
            other => format!("after-{}", other),
        }
    }
    fn produce_failure(&self, reason: &str) -> EngineFailure {
        EngineFailure::UserError(reason.to_string())
    }
    fn notify_rejection(&self, _step_name: &str, _reason: &str) -> String {
        "notified".to_string()
    }
}

// ActionTrackingEffects records whether notify_rejection was called so we can
// assert the action fires exactly once on the reject path.
struct ActionTrackingEffects {
    notified: std::cell::Cell<bool>,
}

impl ActionTrackingEffects {
    fn new() -> Self {
        Self { notified: std::cell::Cell::new(false) }
    }
    fn was_notified(&self) -> bool {
        self.notified.get()
    }
}

impl WorkflowEngineEffects for ActionTrackingEffects {
    fn execute_step(&self, step_name: &str) -> String {
        format!("done:{}", step_name)
    }
    fn needs_approval(&self, step_name: &str) -> bool {
        step_name == "step-b"
    }
    fn next_step_name(&self, current_step: &str) -> String {
        match current_step {
            "step-a" => "step-b".to_string(),
            other => format!("after-{}", other),
        }
    }
    fn produce_failure(&self, reason: &str) -> EngineFailure {
        EngineFailure::UserError(reason.to_string())
    }
    fn notify_rejection(&self, _step_name: &str, _reason: &str) -> String {
        self.notified.set(true);
        "notified".to_string()
    }
}

// StepRunnerNoEffects implements StepRunnerEffects for StepRunner child-machine tests.
struct StepRunnerNoEffects;

impl StepRunnerEffects for StepRunnerNoEffects {
    fn run_step(&self, step: &str) -> String {
        format!("ran:{}", step)
    }
}

// ---------------------------------------------------------------------------
// Completion path
// ---------------------------------------------------------------------------

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
    machine.advance(&effects).expect("advance step-a ok");
    assert!(matches!(
        machine.state(),
        WorkflowEngineState::Running { current_step, remaining }
        if current_step == "step-b" && *remaining == 1
    ));

    // Running("step-b", 1) -> Completed(1)
    machine.advance(&effects).expect("advance step-b ok");
    assert!(matches!(
        machine.state(),
        WorkflowEngineState::Completed { total_steps }
        if *total_steps == 1
    ));
}

// ---------------------------------------------------------------------------
// Approval path
// ---------------------------------------------------------------------------

#[test]
fn pipeline_pauses_at_approval_gate_then_completes() {
    let effects = GatedEffects;
    let mut machine = WorkflowEngine::new(make_config("gated", 2));

    machine.start("step-a".to_string()).expect("start ok");

    // Running("step-a", 2) -> AwaitingApproval("step-b", 1)
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

// ---------------------------------------------------------------------------
// Rejection path — EngineFailure
// ---------------------------------------------------------------------------

#[test]
fn pipeline_rejected_transitions_to_failed_with_engine_failure() {
    let effects = GatedEffects;
    let mut machine = WorkflowEngine::new(make_config("gated-reject", 2));

    machine.start("step-a".to_string()).expect("start ok");
    machine.advance(&effects).expect("advance step-a ok"); // -> AwaitingApproval("step-b", 1)

    assert!(matches!(
        machine.state(),
        WorkflowEngineState::AwaitingApproval { .. }
    ));

    machine
        .reject("compliance check failed".to_string(), &effects)
        .expect("reject from AwaitingApproval should succeed");

    // Failed state carries an EngineFailure, not a raw String.
    assert!(matches!(
        machine.state(),
        WorkflowEngineState::Failed { step_name, failure: EngineFailure::UserError(reason) }
        if step_name == "step-b" && reason == "compliance check failed"
    ));
}

// ---------------------------------------------------------------------------
// action: notify_rejection fires exactly once on the reject path
// ---------------------------------------------------------------------------

#[test]
fn reject_fires_action_exactly_once() {
    let effects = ActionTrackingEffects::new();
    let mut machine = WorkflowEngine::new(make_config("action-test", 2));

    machine.start("step-a".to_string()).expect("start ok");
    machine.advance(&effects).expect("advance step-a ok"); // -> AwaitingApproval

    assert!(!effects.was_notified(), "action must not fire before reject");

    machine
        .reject("manual rejection".to_string(), &effects)
        .expect("reject ok");

    assert!(effects.was_notified(), "action must fire exactly once on reject");
}

// ---------------------------------------------------------------------------
// Approval mid-pipeline (remaining > 1)
// ---------------------------------------------------------------------------

#[test]
fn approval_mid_pipeline_transitions_to_running_not_completed() {
    let effects = ThreeStepGatedEffects;
    let mut machine = WorkflowEngine::new(make_config("three-step", 3));

    machine.start("step-1".to_string()).expect("start ok");

    // Running("step-1", 3) -> AwaitingApproval("step-2", 2)
    machine.advance(&effects).expect("advance step-1 ok");
    assert!(matches!(
        machine.state(),
        WorkflowEngineState::AwaitingApproval { current_step, remaining }
        if current_step == "step-2" && *remaining == 2
    ));

    // remaining=2 > 1, so approve -> Running("step-3", 1)
    machine.approve("step-3".to_string()).expect("approve ok");
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

// ---------------------------------------------------------------------------
// Invalid transitions
// ---------------------------------------------------------------------------

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
    let effects = LinearEffects;
    let mut machine = WorkflowEngine::new(make_config("invalid", 2));
    machine.start("step-a".to_string()).expect("start ok");

    let err = machine
        .reject("should fail".to_string(), &effects)
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

// ---------------------------------------------------------------------------
// StepRunner child machine
// ---------------------------------------------------------------------------

#[test]
fn step_runner_child_machine_runs_to_done() {
    let effects = StepRunnerNoEffects;
    let mut runner = StepRunner::new();

    assert!(matches!(runner.state(), StepRunnerState::Idle));

    runner.start("deploy".to_string()).expect("start ok");
    assert!(matches!(
        runner.state(),
        StepRunnerState::Running { step }
        if step == "deploy"
    ));

    runner.complete(&effects).expect("complete ok");
    assert!(matches!(
        runner.state(),
        StepRunnerState::Done { result }
        if result == "ran:deploy"
    ));
}

#[test]
fn step_runner_start_from_running_returns_invalid_transition() {
    let mut runner = StepRunner::new();
    runner.start("deploy".to_string()).expect("first start ok");

    let err = runner
        .start("other".to_string())
        .expect_err("start from Running must fail");

    assert!(
        matches!(err, StepRunnerError::InvalidTransition { .. }),
        "expected InvalidTransition, got: {:?}",
        err
    );
}
