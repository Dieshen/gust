---
title: "Circuit Breaker"
description: "Stop calling a dependency that is already failing, and probe cautiously before trusting it again."
type: guide
---

# Circuit Breaker

A dependency starts failing. Every caller keeps hammering it, each one paying the full timeout, and the queue behind them grows until your service falls over too. The breaker's job is to fail fast once failures pass a threshold, wait, then let a small number of probes decide whether to reopen the tap.

The three states are the whole idea: **Closed** (traffic flows, failures are counted), **Open** (traffic is refused, a clock is running), **HalfOpen** (a few probes are allowed, and either they succeed enough to close or one failure sends you back).

## The machine

`PaymentBreaker` carries its own thresholds, so a rehydrated breaker still knows its policy.

```gust
machine PaymentBreaker {
    state Closed(failures: i64, threshold: i64)
    state Open(opened_at: i64, timeout_ms: i64)
    state HalfOpen(successes: i64, needed: i64)

    transition fail: Closed -> Closed | Open
    transition check_open: Open -> Open | HalfOpen
    transition succeed_half: HalfOpen -> HalfOpen | Closed

    effect current_time_ms() -> i64

    on fail(ctx) {
        let next_failures = ctx.failures + 1;
        if next_failures >= ctx.threshold {
            goto Open(perform current_time_ms(), 60000);
        } else {
            goto Closed(next_failures, ctx.threshold);
        }
    }

    on check_open(ctx) {
        let elapsed = perform current_time_ms() - ctx.opened_at;
        if elapsed >= ctx.timeout_ms {
            goto HalfOpen(0, 3);
        } else {
            goto Open(ctx.opened_at, ctx.timeout_ms);
        }
    }

    on succeed_half(ctx) {
        let next = ctx.successes + 1;
        if next >= ctx.needed {
            goto Closed(0, 5);
        } else {
            goto HalfOpen(next, ctx.needed);
        }
    }
}
```

Save that as `breaker.gu` and build it. Two things about the shape are worth pausing on.

**`check_open` is a poll, not a timer.** Gust runs no background clock. `Open -> Open` is the "not yet" branch, and something in your host has to fire `check_open` on an interval. That is deliberate: the recovery decision becomes an observable transition instead of hidden timer state.

**There is no `succeed` transition on `Closed`.** A success while closed changes nothing, so it is not modelled. The breaker only hears about failures until it opens.

## What the host implements

One effect, and it is the clock.

```rust "src/breaker_effects.rs"
impl PaymentBreakerEffects for SystemClock {
    fn current_time_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before epoch")
            .as_millis() as i64
    }
}
```

Injecting the clock as an effect is what makes the breaker testable: a fake clock lets you step straight from "just opened" to "recovery window elapsed" without sleeping.

## Driving it

The breaker does not make calls. It records outcomes, and you ask it whether to proceed.

```rust "src/gateway.rs"
let mut breaker = PaymentBreaker::new(0, 5);

// Refuse before you dial.
if matches!(breaker.state(), PaymentBreakerState::Open { .. }) {
    breaker.check_open(&clock)?;               // has the recovery window elapsed?
}

match breaker.state() {
    PaymentBreakerState::Open { .. } => return Err(Rejected::CircuitOpen),
    PaymentBreakerState::HalfOpen { .. } => { /* probe: allow one call */ }
    PaymentBreakerState::Closed { .. } => { /* normal traffic */ }
}

match charge(&card).await {
    Ok(receipt) => {
        if matches!(breaker.state(), PaymentBreakerState::HalfOpen { .. }) {
            breaker.succeed_half()?;
        }
        Ok(receipt)
    }
    Err(e) => {
        if matches!(breaker.state(), PaymentBreakerState::Closed { .. }) {
            breaker.fail(&clock)?;
        }
        Err(e)
    }
}
```

`succeed_half` takes no `effects` argument because its handler performs none — codegen only adds the parameter to transitions that need it.

## The stdlib version

`gust-stdlib/circuit_breaker.gu` is this machine declared as `machine CircuitBreaker<T>`, using bare source-state field references (`failures`, not `ctx.failures`).

The `<T>` is never used by any state field, and that is fatal for the Rust backend: the generated `CircuitBreakerState<T>` fails with `E0392: type parameter 'T' is never used`, and every state assignment then fails inference. The Go output builds and vets clean. Dropping `<T>`, as the recipe does, fixes Rust without changing behaviour — nothing in the machine ever held a `T`.

## Tuning

- **Thresholds belong in state fields, not in literals.** The recipe still hard-codes `60000`, `3`, and `5` inside the handlers, because a `goto` cannot read a field of a state it is leaving for an unrelated purpose. If you need those configurable, carry them through every state the way [retry](./retry.md) carries its policy.
- **`HalfOpen` needs more than one success.** One probe succeeding proves very little; `needed: 3` is a reasonable floor.
- **A failure in `HalfOpen` should reopen.** This machine has no `HalfOpen -> Open` transition. Add one if a failed probe should restart the clock rather than be ignored — `transition fail_half: HalfOpen -> Open`.
- **Pair it with retry, in this order.** The breaker wraps the retry machine, not the other way round. Retrying into an open circuit spends your retry budget on calls you already decided not to make.

## Typed failures

If you want the breaker to record *why* it opened, note that you cannot construct a payload-carrying enum variant inline — `EngineFailure::Timeout(500)` does not parse. Declare an effect that returns the populated variant and `perform` it:

```gust
use std::EngineFailure;

machine PaymentStep {
    state Running(step: String)
    state Failed(step: String, failure: EngineFailure)

    transition reject: Running -> Failed

    effect classify_failure(step: String, reason: String) -> EngineFailure

    on reject(ctx, reason: String) {
        let failure = perform classify_failure(ctx.step, reason);
        goto Failed(ctx.step, failure);
    }
}
```

`EngineFailure` ships as `gust-stdlib/engine_failure.gu`. The `use std::` line resolves the type for validation but is not emitted into the generated Rust, so you also have to build `engine_failure.gu` and bring the enum into scope where the machine is included:

```rust "src/lib.rs"
pub mod failure {
    include!("engine_failure.g.rs");
}

pub mod payment_step {
    pub use super::failure::EngineFailure;
    include!("payment_step.g.rs");
}
```

Two generated files cannot share a module — each emits its own `use serde::{Serialize, Deserialize};` — so give each one a module and re-export across.
