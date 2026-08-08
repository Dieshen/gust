---
title: "Saga"
description: "Run a sequence of steps that must all succeed or all be undone, walking compensation backwards through the steps that already completed."
type: guide
---

# Saga

A booking touches four services: hold the seat, charge the card, issue the ticket, notify the customer. There is no transaction spanning all four. If the third fails, the first two have already happened and somebody has to undo them.

A saga makes that undoing explicit. Forward execution walks a step list; on failure it turns around and walks the *completed* steps backwards, running a compensating action for each. The machine's job is to keep track of exactly how far it got.

## The machine

Gust has no loops, so iteration is a self-transition carrying an index. `Executing -> Executing` advances the cursor; `Compensating -> Compensating` walks it back down.

```gust
machine BookingSaga {
    state Planning(steps: Vec<String>)
    state Executing(steps: Vec<String>, index: i64, completed: Vec<String>)
    state Compensating(completed: Vec<String>, index: i64, reason: String)
    state Committed(completed: Vec<String>)
    state Aborted(reason: String, compensated_count: i64)

    transition begin: Planning -> Executing
    transition execute_next: Executing -> Executing | Compensating | Committed
    transition compensate_next: Compensating -> Compensating | Aborted

    async effect execute_forward(step: String) -> Result<String, String>
    async effect execute_compensate(step: String) -> Result<i64, String>
    effect len(steps: Vec<String>) -> i64
    effect get_step(steps: Vec<String>, index: i64) -> String
    effect push_step(steps: Vec<String>, step: String) -> Vec<String>
    effect empty_steps() -> Vec<String>

    on begin(ctx) {
        goto Executing(ctx.steps, 0, perform empty_steps());
    }

    async on execute_next(ctx) {
        if ctx.index >= perform len(ctx.steps) {
            goto Committed(ctx.completed);
        } else {
            let current = perform get_step(ctx.steps, ctx.index);
            let result = perform execute_forward(current);
            match result {
                Ok(done) => {
                    goto Executing(ctx.steps, ctx.index + 1, perform push_step(ctx.completed, done));
                }
                Err(reason) => {
                    let last = perform len(ctx.completed) - 1;
                    goto Compensating(ctx.completed, last, reason);
                }
            }
        }
    }

    async on compensate_next(ctx) {
        if ctx.index < 0 {
            goto Aborted(ctx.reason, perform len(ctx.completed));
        } else {
            let current = perform get_step(ctx.completed, ctx.index);
            let result = perform execute_compensate(current);
            match result {
                Ok(_) => {
                    goto Compensating(ctx.completed, ctx.index - 1, ctx.reason);
                }
                Err(failure) => {
                    goto Aborted(failure, perform len(ctx.completed) - ctx.index);
                }
            }
        }
    }
}
```

Save as `booking_saga.gu` and build. Three details in that source are load-bearing.

**`len`, `get_step`, and `push_step` are effects because Gust has no method calls.** There is no `steps.len()` and no `steps[index]`. This is the escape hatch working as designed, not a workaround — the stdlib saga declares exactly the same three.

**Every branch is `if`/`else`.** A `goto` inside a bare `if` does not end the handler; it assigns state and execution continues. The stdlib saga relies on falling through after `goto Committed`, and the generated Rust both keeps executing and fails to compile with `borrow of moved value: completed`.

**The `Err` arm binds the length to a `let` first.** Writing `goto Compensating(ctx.completed, perform len(ctx.completed) - 1, reason)` puts a move and a borrow of the same `Vec` in one struct literal, and Rust evaluates fields in written order. Hoisting the length out fixes it.

## What the host implements

The collection effects are trivial; the two async ones are your actual business logic.

```rust "src/saga_effects.rs"
impl BookingSagaEffects for BookingServices {
    async fn execute_forward(&self, step: &str) -> Result<String, String> {
        match step {
            "hold_seat" => self.seats.hold().await.map(|_| step.to_string()),
            "charge_card" => self.payments.charge().await.map(|_| step.to_string()),
            "issue_ticket" => self.tickets.issue().await.map(|_| step.to_string()),
            other => Err(format!("unknown step {other}")),
        }
        .map_err(|e| e.to_string())
    }

    async fn execute_compensate(&self, step: &str) -> Result<i64, String> {
        match step {
            "hold_seat" => self.seats.release().await,
            "charge_card" => self.payments.refund().await,
            "issue_ticket" => self.tickets.void().await,
            other => return Err(format!("no compensation for {other}")),
        }
        .map(|_| 1)
        .map_err(|e| e.to_string())
    }

    fn len(&self, steps: &[String]) -> i64 {
        steps.len() as i64
    }

    fn get_step(&self, steps: &[String], index: i64) -> String {
        steps[index as usize].clone()
    }

    fn push_step(&self, steps: &[String], step: &str) -> Vec<String> {
        let mut next = steps.to_vec();
        next.push(step.to_string());
        next
    }

    fn empty_steps(&self) -> Vec<String> {
        Vec::new()
    }
}
```

`get_step` will panic on an out-of-range index. The machine never produces one — `execute_next` guards with `len` and `compensate_next` guards with `index < 0` — but if you extend the transitions, keep that invariant or make the effect return a sentinel.

## Driving it

The caller fires transitions until the machine lands somewhere terminal.

```rust "src/main.rs"
let mut saga = BookingSaga::new(vec![
    "hold_seat".into(),
    "charge_card".into(),
    "issue_ticket".into(),
]);
saga.begin(&effects)?;

while !matches!(
    saga.state(),
    BookingSagaState::Committed { .. } | BookingSagaState::Aborted { .. }
) {
    if matches!(saga.state(), BookingSagaState::Compensating { .. }) {
        saga.compensate_next(&effects).await?;
    } else {
        saga.execute_next(&effects).await?;
    }
}

match saga.state() {
    BookingSagaState::Committed { completed } => {
        tracing::info!("booked, {} steps", completed.len());
    }
    BookingSagaState::Aborted { reason, compensated_count } => {
        tracing::error!("rolled back {compensated_count} step(s): {reason}");
    }
    _ => unreachable!("loop only exits on a terminal state"),
}
```

Drive the machine with `matches!` rather than from inside a `match` on `state()`. `state()` borrows the machine and a transition needs `&mut self`, so calling one inside a `match` arm is a borrow-checker error.

Every step is a state you can persist between iterations. That is the whole reason to write this as a machine rather than a `for` loop over a `Vec` of undo closures.

## The stdlib version

`gust-stdlib/saga.gu` is `machine Saga<S>`, generic over the step type, using bare field references. It is the reference implementation for index-driven iteration and worth reading.

Both of the things that used to stop it being copy-and-paste ready are fixed on
`master`, and both were real:

- **It did not compile as Rust.** `let steps = steps.clone();` on an unbounded
  `S` gave `&Vec<S>` where `Vec<S>` was expected — the transition impl now
  carries a `Clone` bound.
- **`execute_next` and `compensate_next` fell through their first `if`.** After
  `goto Committed(completed)` the handler continued into `get_step` and
  `execute_forward`, running a step the machine had already declared itself
  finished with. `goto` now returns.

On the published 0.3.0 both still apply; prefer this recipe's `if`/`else` form
there.

## Operational guidance

- **Make forward steps idempotent.** A saga that is resumed after a crash may re-run the step that was in flight.
- **Compensation is not rollback.** You are issuing a refund, not un-charging a card. Model it as a real business action with its own audit trail.
- **Put enough in the step payload to compensate.** `Vec<String>` of step names works when the services can look up their own state. If they cannot, carry a `type BookingStep { name: String, external_id: String }` instead.
- **Mark externally visible compensation as an `action`, not an `effect`.** Replay-aware runtimes checkpoint before an action and will replay an effect freely. At most one action per code path, and it must be the last side-effectful step before the `goto`.
- **Retry inside a step, not around the saga.** A transient network blip should be handled by a [retry](./retry.md) machine within `execute_forward`, so it never reaches the compensation path.
