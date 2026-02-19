# workflow_engine

A realistic workflow execution engine built with a Gust state machine.
Demonstrates approval gates, multi-path transitions, and effect-based step execution.

## What it shows

- A five-state machine with conditional branching driven by effect results
- An approval gate pattern: the `advance` transition routes to `AwaitingApproval`
  when the next step requires human sign-off
- Explicit `approve` and `reject` transitions that consume the waiting state
- Three effects injected at runtime: `execute_step`, `needs_approval`, `next_step_name`
- A deploy pipeline demo (build -> staging -> prod with approval gate)

## Machine design

```
Created -> Running -> Running (loop)
                   -> AwaitingApproval -> Running (approved, more steps)
                                       -> Completed (approved, last step)
                                       -> Failed (rejected)
                   -> Completed (last step, no approval needed)
```

States:

| State              | Fields                                 | Meaning                           |
|--------------------|----------------------------------------|-----------------------------------|
| `Created`          | `config: WorkflowConfig`               | Workflow initialized, not started |
| `Running`          | `current_step: String, remaining: i64` | Actively executing a step         |
| `AwaitingApproval` | `current_step: String, remaining: i64` | Paused, waiting for human gate    |
| `Completed`        | `total_steps: i64`                     | All steps finished successfully   |
| `Failed`           | `step_name: String, reason: String`    | Rejected or errored               |

Effects:

| Effect            | Signature                           | Meaning                          |
|-------------------|-------------------------------------|----------------------------------|
| `execute_step`    | `(step_name: &String) -> String`    | Run a step, return status output |
| `needs_approval`  | `(step_name: &String) -> bool`      | True if step requires sign-off   |
| `next_step_name`  | `(current_step: &String) -> String` | Return the name of the next step |

## Running

```bash
cargo run -p gust-workflow-engine-example
```

## Testing

```bash
cargo test -p gust-workflow-engine-example
```

Nine tests cover:

- Two-step linear pipeline runs to `Completed` without any approval gate
- Pipeline pauses at an approval gate, then `approve` moves to `Completed`
- Pipeline at an approval gate, `reject` moves to `Failed` with reason
- Approval mid-pipeline (remaining > 1) transitions to `Running`, not `Completed`
- Five invalid transition cases each return `WorkflowEngineError::InvalidTransition`

## Project structure

```
examples/workflow_engine/
├── src/
│   ├── workflow.gu       # Gust state machine source
│   └── main.rs           # Effects implementation + deploy pipeline demo
├── tests/
│   └── test.rs           # Integration tests
├── build.rs              # Calls gust_build::compile_gust_files()
└── Cargo.toml
```
