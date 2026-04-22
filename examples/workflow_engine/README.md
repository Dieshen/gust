# workflow_engine

A realistic workflow execution engine built with Gust state machines.

This example demonstrates **`action`**, **`EngineFailure`**, and **`supervises`** — the three
features most relevant to workflow-runtime integration (e.g. Corsac).

## What it shows

### `action` — non-idempotent side effects

`notify_rejection` is declared with the `action` keyword instead of `effect`.
An action is a non-idempotent, externally visible operation (webhook call, email,
payment charge) that a replay-aware runtime must not repeat on replay. The Gust
validator enforces that an action is the last side-effectful step on its code
path before a state transition.

```
action notify_rejection(step_name: String, reason: String) -> String
```

The generated `WorkflowEngineEffects` trait marks this method with a doc comment:
`/// action — not replay-safe / externally visible (#40)`.

### `EngineFailure` — typed workflow failure

The `Failed` state carries an `EngineFailure` value instead of a raw `String`.
`EngineFailure` is defined in `gust-stdlib/engine_failure.gu` and has five
typed variants:

| Variant | Payload | When to use |
|---------|---------|-------------|
| `UserError(reason)` | `String` | Explicit human rejection or bad input |
| `SystemError(reason, attempt)` | `String, i64` | Transient infrastructure fault |
| `IntegrationError(service, status_code, body)` | `String, i64, String` | Downstream API error |
| `Timeout(wall_clock_ms)` | `i64` | Deadline exceeded |
| `Cancelled(requested_by)` | `String` | Explicit cancellation |

In `workflow.gu`:
```
use std::EngineFailure;
...
state Failed(step_name: String, failure: EngineFailure)
```

The `build.rs` copies `engine_failure.gu` from `gust-stdlib` into `src/` so it
is compiled alongside `workflow.gu`. A post-processing step strips the literal
`use std::EngineFailure;` from the generated Rust (which would reference the
Rust `std` crate rather than the Gust stdlib) so the Rust compiler uses the
definition generated from `engine_failure.g.rs`.

### `supervises` — structured concurrency

`WorkflowEngine` declares that it supervises `StepRunner` child machines with
a `one_for_one` restart strategy:

```
machine WorkflowEngine(supervises StepRunner(one_for_one)) {
```

`StepRunner` is a small two-state machine (`Idle -> Running -> Done`) that
models a single pipeline step. The `supervises` clause is the Gust language
surface for structured concurrency; runtime integration (spawning, restarting)
is provided by `gust-runtime::SupervisorRuntime`.

## Machine design

```
Created -> Running -> Running (loop)
                   -> AwaitingApproval -> Running (approved, more steps)
                                       -> Completed (approved, last step)
                                       -> Failed(EngineFailure) (rejected)
                   -> Completed (last step, no approval needed)
                   -> Failed(EngineFailure) (step execution error)
```

States:

| State              | Fields                                      | Meaning                           |
|--------------------|---------------------------------------------|-----------------------------------|
| `Created`          | `config: WorkflowConfig`                    | Workflow initialized, not started |
| `Running`          | `current_step: String, remaining: i64`      | Actively executing a step         |
| `AwaitingApproval` | `current_step: String, remaining: i64`      | Paused, waiting for human gate    |
| `Completed`        | `total_steps: i64`                          | All steps finished successfully   |
| `Failed`           | `step_name: String, failure: EngineFailure` | Rejected or errored               |

Effects and actions:

| Declaration | Kind | Signature | Notes |
|-------------|------|-----------|-------|
| `execute_step` | effect | `(step_name) -> String` | Run a step, return status output |
| `needs_approval` | effect | `(step_name) -> bool` | True if step requires sign-off |
| `next_step_name` | effect | `(current_step) -> String` | Return the name of the next step |
| `produce_failure` | effect | `(reason) -> EngineFailure` | Construct a typed failure value |
| `notify_rejection` | **action** | `(step_name, reason) -> String` | Non-idempotent external notification |

## Running

```bash
cargo run -p gust-workflow-engine-example
```

## Testing

```bash
cargo test -p gust-workflow-engine-example
```

Twelve tests cover:

- Two-step linear pipeline runs to `Completed` without any approval gate
- Pipeline pauses at an approval gate, then `approve` moves to `Completed`
- Pipeline at an approval gate, `reject` moves to `Failed(EngineFailure)`
- Approval mid-pipeline (remaining > 1) transitions to `Running`, not `Completed`
- `notify_rejection` action fires exactly once on the reject path
- Five invalid transition cases each return `WorkflowEngineError::InvalidTransition`
- `StepRunner` child machine runs `Idle -> Running -> Done` correctly
- `StepRunner` rejects a double-start with `InvalidTransition`

## Project structure

```
examples/workflow_engine/
├── src/
│   ├── workflow.gu          # Gust state machine source (WorkflowEngine + StepRunner)
│   ├── workflow.g.rs        # Generated Rust (do not edit manually)
│   ├── engine_failure.gu    # Copied from gust-stdlib by build.rs
│   ├── engine_failure.g.rs  # Generated Rust for EngineFailure enum
│   └── main.rs              # Effects impl + deploy pipeline demo
├── tests/
│   └── test.rs              # Integration tests
├── build.rs                 # Compiles .gu files; copies engine_failure.gu from stdlib
└── Cargo.toml
```
