include!("workflow.g.rs");

// DeployPipelineEffects models a CI/CD deploy pipeline with three stages:
//   1. "build"   — compile and run unit tests (no approval required)
//   2. "staging" — deploy to staging environment (no approval required)
//   3. "prod"    — deploy to production (requires manual approval gate)
//
// The step order is fixed and encoded in next_step_name.
// execute_step simulates work by returning a short status string.
// needs_approval is the approval gate: only "prod" requires human sign-off.
struct DeployPipelineEffects;

impl WorkflowEngineEffects for DeployPipelineEffects {
    fn execute_step(&self, step_name: &String) -> String {
        match step_name.as_str() {
            "build" => "build passed: 42 tests ok".to_string(),
            "staging" => "staging deploy: healthy (latency p99=12ms)".to_string(),
            "prod" => "prod deploy: rollout complete 100%".to_string(),
            other => format!("unknown step '{}': no-op", other),
        }
    }

    fn needs_approval(&self, step_name: &String) -> bool {
        // Only the production deploy requires a human approval gate.
        step_name.as_str() == "prod"
    }

    fn next_step_name(&self, current_step: &String) -> String {
        match current_step.as_str() {
            "build" => "staging".to_string(),
            "staging" => "prod".to_string(),
            // "prod" is the last step; next_step_name is never called when remaining == 0.
            other => format!("after-{}", other),
        }
    }
}

fn main() {
    let effects = DeployPipelineEffects;

    println!("=== Workflow Engine: Deploy Pipeline Demo ===\n");

    // --- Happy path: full pipeline runs to completion with one approval gate ---
    println!("-- Happy path: build -> staging -> await approval -> prod -> Completed --");

    let config = WorkflowConfig {
        name: "release-v2.0".to_string(),
        total_steps: 3,
    };
    let mut machine = WorkflowEngine::new(config);
    println!("Initial state: {:?}", machine.state());

    // Created -> Running("build", 3)
    machine
        .start("build".to_string())
        .expect("start should succeed from Created");
    println!("After start:   {:?}", machine.state());

    // Running("build", 3) -> Running("staging", 2)
    // execute_step("build") passes, next_step="staging", needs_approval("staging")=false
    machine
        .advance(&effects)
        .expect("advance 'build' should succeed");
    println!("After build:   {:?}", machine.state());

    // Running("staging", 2) -> AwaitingApproval("prod", 1)
    // execute_step("staging") passes, next_step="prod", needs_approval("prod")=true
    machine
        .advance(&effects)
        .expect("advance 'staging' should succeed");
    println!("After staging: {:?}", machine.state());

    // Confirm we are waiting for approval before production deploy.
    assert!(
        matches!(machine.state(), WorkflowEngineState::AwaitingApproval { .. }),
        "expected AwaitingApproval after staging"
    );

    // Approval granted — move to Running("prod", 1) then immediately complete via advance.
    // approve transitions to Running since remaining(1) is not > 1.
    // Wait — remaining is 1 here, so approve goes to Completed directly.
    machine
        .approve("prod".to_string())
        .expect("approve should succeed from AwaitingApproval");
    println!("After approve: {:?}", machine.state());

    // remaining was 1, so approve went directly to Completed.
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
    machine2.advance(&effects).expect("advance build ok"); // -> Running("staging", 2)
    machine2.advance(&effects).expect("advance staging ok"); // -> AwaitingApproval("prod", 1)

    println!("Before reject: {:?}", machine2.state());

    machine2
        .reject("security review failed: CVE-2025-1234 unpatched".to_string())
        .expect("reject should succeed from AwaitingApproval");
    println!("After reject:  {:?}", machine2.state());

    assert!(
        matches!(machine2.state(), WorkflowEngineState::Failed { .. }),
        "expected Failed after rejection"
    );

    if let WorkflowEngineState::Failed { step_name, reason } = machine2.state() {
        println!("Pipeline failed at '{}': {}\n", step_name, reason);
    }

    // --- Four-step pipeline: approval gate mid-workflow continues to next step ---
    println!("-- Four-step pipeline: approval gate resumes Running not Completed --");

    // Use a custom effects impl with a longer pipeline to show that approve -> Running path.
    struct FourStepEffects;
    impl WorkflowEngineEffects for FourStepEffects {
        fn execute_step(&self, step_name: &String) -> String {
            format!("executed:{}", step_name)
        }
        fn needs_approval(&self, step_name: &String) -> bool {
            // step-2 requires approval; step-3 does not.
            step_name.as_str() == "step-2"
        }
        fn next_step_name(&self, current_step: &String) -> String {
            match current_step.as_str() {
                "step-1" => "step-2".to_string(),
                "step-2" => "step-3".to_string(),
                "step-3" => "step-4".to_string(),
                other => format!("after-{}", other),
            }
        }
    }

    let four_step_effects = FourStepEffects;
    let config3 = WorkflowConfig {
        name: "four-step-workflow".to_string(),
        total_steps: 4,
    };
    let mut machine3 = WorkflowEngine::new(config3);
    machine3.start("step-1".to_string()).expect("start ok");
    println!("After start:   {:?}", machine3.state()); // Running("step-1", 4)

    // step-1 -> next=step-2, needs_approval("step-2")=true -> AwaitingApproval("step-2", 3)
    machine3.advance(&four_step_effects).expect("advance step-1 ok");
    println!("After step-1:  {:?}", machine3.state()); // AwaitingApproval("step-2", 3)

    assert!(
        matches!(machine3.state(), WorkflowEngineState::AwaitingApproval { .. }),
        "expected AwaitingApproval after step-1"
    );

    // remaining=3 > 1, so approve goes to Running("step-3", 2)
    machine3.approve("step-3".to_string()).expect("approve ok");
    println!("After approve: {:?}", machine3.state()); // Running("step-3", 2)

    assert!(
        matches!(machine3.state(), WorkflowEngineState::Running { .. }),
        "expected Running after approve with remaining > 1"
    );

    // step-3 -> next=step-4, needs_approval("step-4")=false -> Running("step-4", 1)
    machine3.advance(&four_step_effects).expect("advance step-3 ok");
    println!("After step-3:  {:?}", machine3.state()); // Running("step-4", 1)

    // step-4 is last: next_remaining=0 -> Completed(1)
    machine3.advance(&four_step_effects).expect("advance step-4 ok");
    println!("After step-4:  {:?}", machine3.state()); // Completed(1)

    assert!(
        matches!(machine3.state(), WorkflowEngineState::Completed { .. }),
        "expected Completed at end of four-step pipeline"
    );

    println!();

    // --- Invalid transition: reject from Running is not allowed ---
    println!("-- Invalid transition: reject from Running must return an error --");
    let config4 = WorkflowConfig {
        name: "invalid-test".to_string(),
        total_steps: 1,
    };
    let mut machine4 = WorkflowEngine::new(config4);
    machine4.start("build".to_string()).expect("start ok");

    let err = machine4
        .reject("should fail".to_string())
        .expect_err("reject from Running must return an error");
    println!("Got expected error: {}\n", err);

    println!("All demo scenarios completed successfully.");
}
