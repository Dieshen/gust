# Gust modeling patterns

How to express things the expression language can't do directly. Every pattern here is drawn from `gust-stdlib/` or the repo's examples.

## Iteration as a self-transition

Gust has no loops. Model iteration as a transition from a state back to itself, carrying the cursor in a state field. The saga machine walks a step list this way:

```gust
state Executing(steps: Vec<S>, index: i64, completed: Vec<S>)
transition execute_next: Executing -> Executing | Compensating | Committed

async on execute_next() {
    if index >= perform len(steps) {
        goto Committed(completed);
    }
    let current = perform get_step(steps, index);
    let result = perform execute_forward(current);
    match result {
        Ok(done) => {
            goto Executing(steps, index + 1, perform push_step(completed, done));
        }
        Err(err) => {
            goto Compensating(completed, perform len(completed) - 1, err);
        }
    }
}
```

The loop body is the handler; the caller drives it by firing the transition until it lands somewhere terminal. This is more verbose than a `for`, and that's the deliberate trade: every step is an observable state, which is what makes the machine replayable and inspectable.

A multi-phase loop uses a cycle of states rather than one. Retry alternates `Attempting -> Waiting -> Attempting` so the delay is its own observable state rather than a blocking sleep hidden inside a handler.

## Threading state through transitions

There is no ambient context. Anything a later state needs must be re-passed in every `goto`, which is why `Retry` carries its whole configuration through five states:

```gust
state Attempting(attempt: i64, max_attempts: i64, base_delay_ms: i64, max_delay_ms: i64, jitter_pct: i64)
state Waiting(attempt: i64, delay_ms: i64, max_attempts: i64, base_delay_ms: i64, max_delay_ms: i64, jitter_pct: i64)
```

This gets tedious fast. Two ways to keep it manageable:

- **Group configuration into one `type`** and carry a single field: `state Attempting(attempt: i64, config: RetryConfig)`. Field access still works through it (`ctx.config.max_attempts`), and adding a knob doesn't touch every state.
- **Accept the repetition when the fields are genuinely independent**, as the stdlib does — explicit fields make each state's contract readable at a glance.

Prefer the grouped form for anything beyond about three carried values.

## Effects as pure helpers

Because there are no method calls, ordinary operations become effect declarations:

```gust
effect len(steps: Vec<S>) -> i64
effect get_step(steps: Vec<S>, index: i64) -> S
effect push_step(steps: Vec<S>, step: S) -> Vec<S>
effect current_time_ms() -> i64
effect compute_backoff(base_delay_ms: i64, attempt: i64, max_delay_ms: i64, jitter_pct: i64) -> i64
```

Two judgment calls:

- **Keep them coarse.** Each effect is a trait method someone implements in Rust *and* Go. `compute_backoff` as one effect beats four effects for multiply, min, random, and clamp.
- **Push real logic down, not control flow up.** `compute_backoff` computing a jittered delay is good. An effect that decides which state to go to next is hiding the machine's logic in the host language, which defeats the point.

## Fallible operations

**If you target Go, do not use `Result` + `match`.** It is the natural Rust shape and the stdlib uses it, but the Go backend lowers a `Result`-returning effect into Go's `(value, err)` idiom with an automatic early return, so a following `Ok`/`Err` match has nothing to match on. The emitted Go does not compile:

```
resmatch.g.go:88:8: undefined: Ok
resmatch.g.go:92:8: undefined: v
```

`gust check` passes, so nothing warns you. Verified against the real Go toolchain.

**Portable form — a plain flag or sentinel plus `if`/`else`:**

```gust
async effect send_webhook(payload: WebhookPayload) -> bool

async on process_next(ctx) {
    let delivered = perform send_webhook(current);
    if delivered {
        goto Processing(ctx.payloads, ctx.index + 1, 1, ctx.failed);
    } else {
        goto Failed(current.id);
    }
}
```

When you need a reason rather than a bare success flag, return the reason string and treat an agreed empty value as success, or declare two effects — one that attempts and one that reports the last error.

**Rust-only form**, fine if Rust is your sole target:

```gust
async effect execute_operation() -> Result<T, String>

async on run() {
    let result = perform execute_operation();
    match result {
        Ok(value) => { goto Succeeded(value, attempt); }
        Err(err) => { goto Failed(err, attempt); }
    }
}
```

Either way, declare the transition with all its outcomes: `transition run: Attempting -> Waiting | Succeeded | Failed`.

One quirk to expect: the validator warns "handler has code paths that don't end with a goto" on a handler whose every `match` arm ends in `goto`, because its terminator analysis doesn't descend into match arms. It's a false positive; the code is correct.

## Non-idempotent operations

Mark anything externally visible and unrepeatable as an `action`, and put it last on its path before the `goto`:

```gust
action notify_rejection(step_name: String, reason: String) -> String

on reject(ctx, reason: String) {
    let failure = perform produce_failure(reason);
    perform notify_rejection(ctx.current_step, reason);
    goto Failed(ctx.current_step, failure);
}
```

The validator enforces at most one action per code path, and that it's the last side-effectful step. Replay-aware runtimes checkpoint immediately before an action, so anything after it would run twice on replay.

## Timeouts — narrower than they look

```gust
transition run: Idle -> Done timeout 30s
```

**`timeout` is a watchdog on handler execution, not a clock on the state.** Codegen wraps the handler body in `tokio::time::timeout`, and on expiry the transition returns `Err({Machine}Error::Failed { reason: "transition 'run' timed out after ..." })`. There is **no timeout target state** and no state change — the machine stays where it was and the caller gets an `Err`. Declaring `-> Done | TimedOut` gains you nothing; the timeout path never reaches `TimedOut`.

Two side effects of adding it: the generated transition method becomes `async` even if the handler is synchronous, and `use tokio;` is added to the generated prelude.

So use `timeout` for "this operation must not hang" — bounding a slow effect. Units are `ms`, `s`, `m`, `h`.

### Modeling elapsed time in a state

For "auto-resolve 30 minutes after acknowledgement" — a deadline while the machine sits idle — `timeout` is the wrong tool, because nothing is executing to time out. Stamp the entry time into the state and have the host poll a self-transition, which is the `circuit_breaker.gu` pattern:

```gust
state Acknowledged(alert: Alert, acknowledged_at_ms: i64)

transition check_auto_resolve: Acknowledged -> Acknowledged | Resolved

effect current_time_ms() -> i64

on check_auto_resolve(ctx) {
    let elapsed = perform current_time_ms() - ctx.acknowledged_at_ms;
    if elapsed >= 1800000 {
        goto Resolved(ctx.alert);
    } else {
        goto Acknowledged(ctx.alert, ctx.acknowledged_at_ms);
    }
}
```

Gust runs no background clock, so the host application drives this — typically a `tokio::time::interval` calling `check_auto_resolve()`. That's app wiring, not something codegen provides. The upside is that the deadline check is an observable transition rather than hidden timer state.

For a delay you control explicitly — backoff, throttling — use a `Waiting` state with a `sleep_ms` effect, as `retry.gu` does, so the wait is a visible state rather than a blocking sleep inside a handler.

## Supervision

```gust
machine StepRunner {
    state Idle(step: String)
    state Done(step: String)

    transition run: Idle -> Done

    on run(ctx) {
        goto Done(ctx.step);
    }
}

machine Engine(supervises StepRunner(one_for_one)) {
    state Ready(first_step: String, total: i64)
    state Running(step: String, total: i64)

    transition start: Ready -> Running

    on start(ctx) {
        spawn StepRunner(ctx.first_step);
        goto Running(ctx.first_step, ctx.total);
    }
}
```

The supervised machine must be declared — since 1.0 `supervises` and `spawn`
both check the name, and `spawn` checks the argument count against the child's
constructor, which is the fields of its *first* state.

Strategies: `one_for_one` restarts only the failed child; `one_for_all` restarts every child; `rest_for_one` restarts the failed child and everything started after it. Pick `one_for_one` when children are independent, `one_for_all` when they share state that a partial restart would leave inconsistent.

## Channels

**Channels are currently broken on the Rust backend.** A `sends` annotation emits `pub fn send_<channel>(&self, …)` at *module scope*, outside any `impl`, which rustc rejects:

```
error: `self` parameter is only allowed in associated functions
```

Verified against master. So a machine declaring `sends` produces Rust that does not compile, and no test catches it — the compiler's own `channel` fixture has no `sends` clause. Treat everything below as the intended design rather than working behaviour, and check the current state before relying on it.

```gust
channel Orders: Order (capacity: 64, mode: mpsc)

machine Producer(sends Orders) {
    on emit(ctx) {
        send Orders(ctx.order);
        goto Idle;
    }
}

machine Consumer(receives Orders) { ... }
```

`mpsc` for work distribution (each message to one consumer), `broadcast` for fan-out (every consumer sees every message). `send` takes exactly one argument, and the channel declaration takes no trailing semicolon.

## The stdlib

Machines in `gust-stdlib/`, worth reading before writing your own version of one — with the caveat that **they are Rust-only**. Each uses at least one construct the Go backend miscompiles (bare field references, `Result`-matching, generics); `rate_limiter.gu` uses two. Read them for idiom, not as portability models.

| Machine | Shape |
|---|---|
| `circuit_breaker.gu` | `Closed / Open / HalfOpen` with a failure threshold and a time-based recovery probe |
| `retry.gu` | `Ready / Attempting / Waiting / Succeeded / Failed` with computed backoff and jitter |
| `saga.gu` | Forward execution with compensating rollback — the reference for index-driven iteration |
| `rate_limiter.gu` | Token-bucket admission control |
| `health_check.gu` | Periodic probe with healthy/unhealthy transitions |
| `request_response.gu` | Correlated request/reply lifecycle |
| `engine_failure.gu` | The `EngineFailure` enum — an example of working around positional-only enum payloads |

Import with `use std::EngineFailure;`.

## Naming

The repo's conventions, worth matching:

- **States**: PascalCase, named for the condition the machine is *in* — `Validated`, `AwaitingApproval`, `Compensating`. Not verbs.
- **Transitions**: snake_case verbs for the event — `validate`, `execute_next`, `check_open`.
- **A terminal failure state** carrying a reason (`Failed(reason: String)`) rather than encoding failure in a boolean field. It makes the state graph and the generated diagram honest.
- **Ctx placeholder types**: `{Transition}Ctx` in PascalCase — `ValidateCtx`, `StartCtx`. Leave them undeclared.
