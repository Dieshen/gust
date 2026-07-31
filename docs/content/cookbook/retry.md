---
title: "Retry"
description: "Retry a transient failure a bounded number of times, with the wait between attempts modelled as its own observable state."
type: guide
---

# Retry

An upload fails because a socket dropped. Trying again would work. Trying again forever, immediately, from every instance you run, would take the dependency down.

Retry needs three things a bare loop does not give you: a bounded attempt count, a delay that grows, and jitter so a thousand clients do not wake up together. Gust has no loops, so the retry cycle becomes a pair of states — `Attempting` and `Waiting` — that hand off to each other until one of them lands somewhere terminal.

The payoff is that the wait is a *state*. A machine sitting in `Waiting` can be serialised, inspected, and resumed. A `tokio::time::sleep` inside a handler cannot.

## The machine

`UploadRetry` carries its entire policy through every state, because Gust has no ambient context — anything a later state needs must be re-passed in the `goto`.

```gust
machine UploadRetry {
    state Ready(max_attempts: i64, base_delay_ms: i64, max_delay_ms: i64, jitter_pct: i64)
    state Attempting(attempt: i64, max_attempts: i64, base_delay_ms: i64, max_delay_ms: i64, jitter_pct: i64)
    state Waiting(attempt: i64, delay_ms: i64, max_attempts: i64, base_delay_ms: i64, max_delay_ms: i64, jitter_pct: i64)
    state Succeeded(value: String, attempts: i64)
    state Failed(error: String, attempts: i64)

    transition begin: Ready -> Attempting
    transition run: Attempting -> Waiting | Succeeded | Failed
    transition wait_complete: Waiting -> Attempting

    async effect execute_operation() -> Result<String, String>
    async effect sleep_ms(duration_ms: i64) -> i64
    effect compute_backoff(base_delay_ms: i64, attempt: i64, max_delay_ms: i64, jitter_pct: i64) -> i64

    on begin(ctx: BeginCtx) {
        goto Attempting(1, ctx.max_attempts, ctx.base_delay_ms, ctx.max_delay_ms, ctx.jitter_pct);
    }

    async on run(ctx: RunCtx) {
        let result = perform execute_operation();
        match result {
            Ok(value) => {
                goto Succeeded(value, ctx.attempt);
            }
            Err(err) => {
                if ctx.attempt >= ctx.max_attempts {
                    goto Failed(err, ctx.attempt);
                } else {
                    let delay = perform compute_backoff(ctx.base_delay_ms, ctx.attempt, ctx.max_delay_ms, ctx.jitter_pct);
                    goto Waiting(ctx.attempt, delay, ctx.max_attempts, ctx.base_delay_ms, ctx.max_delay_ms, ctx.jitter_pct);
                }
            }
        }
    }

    async on wait_complete(ctx: WaitCtx) {
        perform sleep_ms(ctx.delay_ms);
        goto Attempting(ctx.attempt + 1, ctx.max_attempts, ctx.base_delay_ms, ctx.max_delay_ms, ctx.jitter_pct);
    }
}
```

Save it as `upload_retry.gu` and build. `gust check` will warn that `run` has code paths that don't end with a `goto` — that is the terminator analysis failing to descend into `match` arms, and every arm here does transition.

Note `perform sleep_ms(ctx.delay_ms);` is written as a statement, not `let _ = perform …`. Binding a result you do not read is a hard error in Go and a `clippy -D warnings` error in Rust on 0.3.0. The statement form is always safe.

## What the host implements

Three effects, and the backoff maths is the interesting one.

```rust "src/retry_effects.rs"
impl UploadRetryEffects for Uploader {
    async fn execute_operation(&self) -> Result<String, String> {
        self.put_object().await.map_err(|e| e.to_string())
    }

    async fn sleep_ms(&self, duration_ms: i64) -> i64 {
        tokio::time::sleep(Duration::from_millis(duration_ms as u64)).await;
        duration_ms
    }

    fn compute_backoff(
        &self,
        base_delay_ms: i64,
        attempt: i64,
        max_delay_ms: i64,
        jitter_pct: i64,
    ) -> i64 {
        let exponential = base_delay_ms.saturating_mul(1 << (attempt - 1).min(30));
        let capped = exponential.min(max_delay_ms);
        let spread = capped * jitter_pct / 100;
        capped - spread + self.rng.gen_range(0..=2 * spread)
    }
}
```

`compute_backoff` is one coarse effect rather than four fine ones for multiply, min, random, and clamp. That is the right granularity: every effect becomes a trait method somebody implements in both Rust and Go, so keep them worth implementing.

It is also the boundary you should not cross. An effect that computes a delay is good. An effect that decides *which state to go to next* has moved the machine's logic into the host language, which defeats the point of writing it in Gust.

## Driving it

The caller runs the cycle. Nothing in Gust drives a machine on its own.

```rust "src/main.rs"
let mut retry = UploadRetry::new(5, 200, 30_000, 20);
retry.begin()?;

while !matches!(
    retry.state(),
    UploadRetryState::Succeeded { .. } | UploadRetryState::Failed { .. }
) {
    if matches!(retry.state(), UploadRetryState::Waiting { .. }) {
        retry.wait_complete(&effects).await?;
    } else {
        retry.run(&effects).await?;
    }
}

match retry.state() {
    UploadRetryState::Succeeded { value, attempts } => {
        tracing::info!("uploaded after {attempts} attempt(s)");
        Ok(value.clone())
    }
    UploadRetryState::Failed { error, attempts } => {
        Err(format!("gave up after {attempts}: {error}"))
    }
    _ => unreachable!("loop only exits on a terminal state"),
}
```

`begin` takes no `effects` argument because its handler performs none. The loop is in the host, where it belongs; the machine only decides where each step leads.

Use `matches!` rather than driving the machine from inside a `match` on `state()`. `state()` returns a borrow of the machine, and calling a transition needs `&mut self` — a transition call inside a `match` arm is a borrow-checker error.

## The stdlib version

`gust-stdlib/retry.gu` is this machine as `machine Retry<T>`, with `Succeeded(value: T, attempts: i64)` and `execute_operation() -> Result<T, String>`. It uses bare source-state field references (`attempt`, not `ctx.attempt`).

This is the one stdlib machine whose generated Rust currently compiles. `T` appears in a state field and only ever arrives owned from an effect, so codegen never needs the `Clone` bound it does not emit. The Go output also builds and vets clean. Use `Retry<T>` directly if the generic form suits you; the recipe above just fixes the value type to `String` and uses the `ctx.` form.

## Tuning

- **Cap the delay.** `max_delay_ms` exists so exponential growth does not turn a 5-attempt policy into a 40-minute one.
- **Jitter is not optional at scale.** Without it, every client that failed at the same moment retries at the same moment.
- **Keep the budget finite.** `max_attempts` is what stops retry from becoming a denial-of-service tool aimed at your own dependency.
- **Group the policy if you extend it.** Five carried fields is already tedious, and every new knob touches every `goto`. Put them in one `type RetryConfig { ... }` and carry a single field; `ctx.config.max_attempts` still works.
- **Retry goes inside a saga step, and inside a circuit breaker.** A transient failure retried successfully should never reach the [saga](./saga.md)'s compensation path, and retrying into an [open circuit](./circuit_breaker.md) wastes the budget.
