// Each generated file is wrapped in its own module to avoid duplicate
// `use serde::{Serialize, Deserialize}` imports when multiple machines
// are included in the same crate.
mod engine_failure_types {
    include!("engine_failure.g.rs");
}

mod workflow_machine {
    pub use super::engine_failure_types::EngineFailure;
    include!("workflow.g.rs");
}

// Re-export everything so the rest of main can use unqualified names.
pub use engine_failure_types::EngineFailure;
pub use workflow_machine::{
    StepRunner, StepRunnerEffects, StepRunnerError, StepRunnerState,
    WorkflowConfig, WorkflowEngine, WorkflowEngineEffects, WorkflowEngineError,
    WorkflowEngineState,
};

// DeployPipelineEffects models a CI/CD deploy pipeline with three stages:
//   1. "build"   — compile and run unit tests (no approval required)
//   2. "staging" — deploy to staging environment (no approval required)
//   3. "prod"    — deploy to production (requires manual approval gate)
//
// notify_rejection demonstrates the action contract: it is called once as
// the last side-effectful step before transitioning to Failed. Replay-aware
// runtimes (Corsac) will not re-execute it on replay.
struct DeployPipelineEffects;

impl WorkflowEngineEffects for DeployPipelineEffects {
    fn execute_step(&self, step_name: &str) -> String {
        match step_name {
            "build" => "build passed: 42 tests ok".to_string(),
            "staging" => "staging deploy: healthy (latency p99=12ms)".to_string(),
            "prod" => "prod deploy: rollout complete 100%".to_string(),
            other => format!("unknown step '{}': no-op", other),
        }
    }

    fn needs_approval(&self, step_name: &str) -> bool {
        // Only the production deploy requires a human approval gate.
        step_name == "prod"
    }

    fn next_step_name(&self, current_step: &str) -> String {
        match current_step {
            "build" => "staging".to_string(),
            "staging" => "prod".to_string(),
            // "prod" is the last step; next_step_name is never called when remaining == 0.
            other => format!("after-{}", other),
        }
    }

    fn produce_failure(&self, reason: &str) -> EngineFailure {
        // Wrap rejection reasons as UserError — the reviewer made an explicit decision.
        EngineFailure::UserError(reason.to_string())
    }

    fn notify_rejection(&self, step_name: &str, reason: &str) -> String {
        // In production this would send a webhook or email. Here we just log.
        println!("[action] rejection notified: step={} reason={}", step_name, reason);
        format!("notified:{}:{}", step_name, reason)
    }
}

fn main() {
    let effects = DeployPipelineEffects;

    println!("=== Workflow Engine: Deploy Pipeline Demo ===");
    println!("    Demonstrates: action, EngineFailure, supervises\n");

    // --- Happy path: full pipeline runs to completion with one approval gate ---
    println!("-- Happy path: build -> staging -> await approval -> prod -> Completed --");

    let config = WorkflowConfig {
        name: "release-v2.0".to_string(),
        total_steps: 3,
    };
    let mut machine = WorkflowEngine::new(config);
    println!("Initial state: {:?}", machine.state());

    machine
        .start("build".to_string())
        .expect("start should succeed from Created");
    println!("After start:   {:?}", machine.state());

    machine
        .advance(&effects)
        .expect("advance 'build' should succeed");
    println!("After build:   {:?}", machine.state());

    machine
        .advance(&effects)
        .expect("advance 'staging' should succeed");
    println!("After staging: {:?}", machine.state());

    assert!(
        matches!(machine.state(), WorkflowEngineState::AwaitingApproval { .. }),
        "expected AwaitingApproval after staging"
    );

    machine
        .approve("prod".to_string())
        .expect("approve should succeed from AwaitingApproval");
    println!("After approve: {:?}", machine.state());

    assert!(
        matches!(machine.state(), WorkflowEngineState::Completed { .. }),
        "expected Completed after approve with remaining==1"
    );

    if let WorkflowEngineState::Completed { total_steps } = machine.state() {
        println!("Pipeline completed: {} steps executed\n", total_steps);
    }

    // --- Rejection path: approval denied sends pipeline to Failed ---
    println!("-- Rejection path: staging ok -> prod awaiting -> rejected -> Failed --");

    let config2 = WorkflowConfig {
        name: "release-v2.1".to_string(),
        total_steps: 3,
    };
    let mut machine2 = WorkflowEngine::new(config2);

    machine2.start("build".to_string()).expect("start ok");
    machine2.advance(&effects).expect("advance build ok");
    machine2.advance(&effects).expect("advance staging ok");

    println!("Before reject: {:?}", machine2.state());

    machine2
        .reject("security review failed: CVE-2025-1234 unpatched".to_string(), &effects)
        .expect("reject should succeed from AwaitingApproval");
    println!("After reject:  {:?}", machine2.state());

    assert!(
        matches!(machine2.state(), WorkflowEngineState::Failed { .. }),
        "expected Failed after rejection"
    );

    if let WorkflowEngineState::Failed { step_name, failure } = machine2.state() {
        println!("Pipeline failed at '{}': {:?}\n", step_name, failure);
    }

    // --- Invalid transition: reject from Running is not allowed ---
    println!("-- Invalid transition: reject from Running must return an error --");
    let config4 = WorkflowConfig {
        name: "invalid-test".to_string(),
        total_steps: 1,
    };
    let mut machine4 = WorkflowEngine::new(config4);
    machine4.start("build".to_string()).expect("start ok");

    let err = machine4
        .reject("should fail".to_string(), &effects)
        .expect_err("reject from Running must return an error");
    println!("Got expected error: {}\n", err);

    println!("All demo scenarios completed successfully.");
}
