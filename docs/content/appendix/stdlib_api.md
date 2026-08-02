---
title: "Stdlib API"
description: "Every machine and type shipped in gust-stdlib: states, transitions, and the effects you must implement."
type: reference
---

# Stdlib API

`gust-stdlib` ships six reusable machines and one type as embedded `.gu` **source strings**, not as compiled code. Each is a `pub const &str` produced by `include_str!`, so you feed it to the compiler exactly as you would your own file.

```rust
let sources = gust_stdlib::all_sources();     // [(&str, &str); 7]

for (filename, source) in &sources {
    println!("{filename}: {} bytes", source.len());
}
```

The individual constants are `CIRCUIT_BREAKER`, `RETRY`, `SAGA`, `RATE_LIMITER`, `HEALTH_CHECK`, `REQUEST_RESPONSE`, and `ENGINE_FAILURE`.

Because they are source, you can also copy a machine into your own `.gu` and edit it. That is often the right move: the constants below (a 60-second open timeout, a threshold of five, three half-open successes) are baked into the handler bodies rather than exposed as configuration.

## Reading these pages

Each machine lists its states, its transitions, and the effects you must implement. Effects are the contract: every one becomes a method on a generated `{Machine}Effects` trait in Rust, or an interface in Go, and the machine does not compile until you supply them.

::: callout warning "The stdlib is idiomatic, not portable"
Every one of these machines reads its source-state fields by bare name (`failures`, not `ctx.failures`), and several match on `Result` or take a generic parameter. All three used to break the Go backend outright; they now lower correctly and all seven sources generate Go that `go build` accepts. The `wasm` backend still cannot express any of them, because `#[wasm_bindgen]` rejects type parameters. Prefer the explicit `ctx:` form in your own machines regardless — see [Known Limitations](known_limitations.md).
:::

## CircuitBreaker

Protects calls to a flaky dependency. Failures are counted while the circuit is `Closed`; crossing the threshold opens it, an open circuit refuses work until a timeout elapses, and a half-open circuit lets a few probes through before closing again.

Generic over `T`, the protected call's context type.

| | |
| --- | --- |
| **States** | `Closed(failures, threshold)`, `Open(opened_at, timeout_ms)`, `HalfOpen(successes, needed)` |
| **Transitions** | `fail: Closed -> Closed \| Open`, `check_open: Open -> Open \| HalfOpen`, `succeed_half: HalfOpen -> HalfOpen \| Closed` |
| **Effects** | `current_time_ms() -> i64` |

```gust
machine CircuitBreaker<T> {
    state Closed(failures: i64, threshold: i64)
    state Open(opened_at: i64, timeout_ms: i64)
    state HalfOpen(successes: i64, needed: i64)

    transition fail: Closed -> Closed | Open
    transition check_open: Open -> Open | HalfOpen
    transition succeed_half: HalfOpen -> HalfOpen | Closed

    effect current_time_ms() -> i64

    on fail() {
        let next_failures = failures + 1;
        if next_failures >= threshold {
            goto Open(perform current_time_ms(), 60000);
        } else {
            goto Closed(next_failures, threshold);
        }
    }

    on check_open() {
        let elapsed = perform current_time_ms() - opened_at;
        if elapsed >= timeout_ms {
            goto HalfOpen(0, 3);
        } else {
            goto Open(opened_at, timeout_ms);
        }
    }

    on succeed_half() {
        let next = successes + 1;
        if next >= needed {
            goto Closed(0, 5);
        } else {
            goto HalfOpen(next, needed);
        }
    }
}
```

Note what the machine does *not* model: success while `Closed`, and failure while `HalfOpen`. The failure counter only ever climbs, and a probe that fails has no transition. Both are your responsibility — reconstruct the machine in `Closed(0, threshold)` on a success, and treat a failed probe as a fresh `Open`.

The literals are fixed: `60000` ms open timeout, `3` half-open successes needed, and a threshold of `5` on re-closing. Copy the source and edit them if those do not suit.

## Retry

Retry with exponential backoff and jitter. `Ready` holds the policy, `Attempting` runs the operation, `Waiting` sleeps between attempts, and the run ends in `Succeeded` or `Failed`.

Generic over `T`, the value returned on success.

| | |
| --- | --- |
| **States** | `Ready(...policy)`, `Attempting(attempt, ...policy)`, `Waiting(attempt, delay_ms, ...policy)`, `Succeeded(value, attempts)`, `Failed(error, attempts)` |
| **Transitions** | `begin: Ready -> Attempting`, `run: Attempting -> Waiting \| Succeeded \| Failed`, `wait_complete: Waiting -> Attempting` |
| **Effects** | `async execute_operation() -> Result<T, String>`, `async sleep_ms(duration_ms: i64) -> i64`, `compute_backoff(base_delay_ms, attempt, max_delay_ms, jitter_pct) -> i64` |

```gust
machine Retry<T> {
    state Ready(max_attempts: i64, base_delay_ms: i64, max_delay_ms: i64, jitter_pct: i64)
    state Attempting(attempt: i64, max_attempts: i64, base_delay_ms: i64, max_delay_ms: i64, jitter_pct: i64)
    state Waiting(attempt: i64, delay_ms: i64, max_attempts: i64, base_delay_ms: i64, max_delay_ms: i64, jitter_pct: i64)
    state Succeeded(value: T, attempts: i64)
    state Failed(error: String, attempts: i64)

    transition begin: Ready -> Attempting
    transition run: Attempting -> Waiting | Succeeded | Failed
    transition wait_complete: Waiting -> Attempting

    async effect execute_operation() -> Result<T, String>
    async effect sleep_ms(duration_ms: i64) -> i64
    effect compute_backoff(base_delay_ms: i64, attempt: i64, max_delay_ms: i64, jitter_pct: i64) -> i64

    on begin() {
        goto Attempting(1, max_attempts, base_delay_ms, max_delay_ms, jitter_pct);
    }

    async on run() {
        let result = perform execute_operation();
        match result {
            Ok(value) => {
                goto Succeeded(value, attempt);
            }
            Err(err) => {
                if attempt >= max_attempts {
                    goto Failed(err, attempt);
                } else {
                    let delay = perform compute_backoff(base_delay_ms, attempt, max_delay_ms, jitter_pct);
                    goto Waiting(attempt, delay, max_attempts, base_delay_ms, max_delay_ms, jitter_pct);
                }
            }
        }
    }

    async on wait_complete() {
        perform sleep_ms(delay_ms);
        goto Attempting(attempt + 1, max_attempts, base_delay_ms, max_delay_ms, jitter_pct);
    }
}
```

The backoff curve is entirely yours — `compute_backoff` receives the base delay, the attempt number, the cap, and the jitter percentage, and the machine only stores what it returns. `sleep_ms` returns `i64` rather than `()` because it predates nothing in particular; the return value is unused, and the handler calls it in statement form.

Every field of the policy is threaded through every state, which is why the state signatures are long. That is the cost of a language with no ambient machine-level fields.

## Saga

Runs a sequence of steps forward, and on failure compensates the completed ones in reverse. `Executing` walks the list by index; `Compensating` walks the completed list backwards.

Generic over `S`, the step type.

| | |
| --- | --- |
| **States** | `Planning(steps)`, `Executing(steps, index, completed)`, `Compensating(completed, index, reason)`, `Committed(completed)`, `Aborted(reason, compensated_count)` |
| **Transitions** | `begin: Planning -> Executing`, `execute_next: Executing -> Executing \| Compensating \| Committed`, `compensate_next: Compensating -> Compensating \| Aborted` |
| **Effects** | `async execute_forward(step: S) -> Result<S, String>`, `async execute_compensate(step: S) -> Result<i64, String>`, `len(steps: Vec<S>) -> i64`, `get_step(steps: Vec<S>, index: i64) -> S`, `push_step(steps: Vec<S>, step: S) -> Vec<S>`, `empty_steps() -> Vec<S>` |

```gust
machine Saga<S> {
    state Planning(steps: Vec<S>)
    state Executing(steps: Vec<S>, index: i64, completed: Vec<S>)
    state Compensating(completed: Vec<S>, index: i64, reason: String)
    state Committed(completed: Vec<S>)
    state Aborted(reason: String, compensated_count: i64)

    transition begin: Planning -> Executing
    transition execute_next: Executing -> Executing | Compensating | Committed
    transition compensate_next: Compensating -> Compensating | Aborted

    async effect execute_forward(step: S) -> Result<S, String>
    async effect execute_compensate(step: S) -> Result<i64, String>
    effect len(steps: Vec<S>) -> i64
    effect get_step(steps: Vec<S>, index: i64) -> S
    effect push_step(steps: Vec<S>, step: S) -> Vec<S>
    effect empty_steps() -> Vec<S>

    on begin() {
        goto Executing(steps, 0, perform empty_steps());
    }

    async on execute_next() {
        if index >= perform len(steps) {
            goto Committed(completed);
        }

        let current = perform get_step(steps, index);
        let result = perform execute_forward(current);
        match result {
            Ok(done) => {
                let next_completed = perform push_step(completed, done);
                goto Executing(steps, index + 1, next_completed);
            }
            Err(err) => {
                goto Compensating(completed, perform len(completed) - 1, err);
            }
        }
    }

    async on compensate_next() {
        if index < 0 {
            goto Aborted(reason, perform len(completed));
        }

        let current = perform get_step(completed, index);
        let result = perform execute_compensate(current);
        match result {
            Ok(_) => {
                goto Compensating(completed, index - 1, reason);
            }
            Err(err) => {
                goto Aborted(err, perform len(completed) - index);
            }
        }
    }
}
```

Saga is the clearest illustration of the effect escape hatch. `len`, `get_step`, `push_step`, and `empty_steps` exist purely because Gust has no method calls, no indexing, and no collection literals — the four of them are `.len()`, `[i]`, `.push()`, and `vec![]` in disguise. Four trivial trait methods buys you a language small enough to lower to two backends.

Iteration is a self-transition. `execute_next` is called repeatedly; each call advances `index` by one and returns to `Executing` until the index reaches the length. There is no loop because there is no loop construct.

## RateLimiter

A token bucket in two states. `acquire` spends a token or exhausts the bucket; `refill` restores it to full.

Generic over `K`, the rate-limit key type.

| | |
| --- | --- |
| **States** | `Available(tokens, max_tokens)`, `Exhausted(retry_after_ms, max_tokens)` |
| **Transitions** | `acquire: Available -> Available \| Exhausted`, `refill: Exhausted -> Available` |
| **Effects** | `now_ms() -> i64` |

```gust
machine RateLimiter<K> {
    state Available(tokens: i64, max_tokens: i64)
    state Exhausted(retry_after_ms: i64, max_tokens: i64)

    transition acquire: Available -> Available | Exhausted
    transition refill: Exhausted -> Available

    effect now_ms() -> i64

    on acquire() {
        if tokens > 0 {
            goto Available(tokens - 1, max_tokens);
        } else {
            goto Exhausted(perform now_ms(), max_tokens);
        }
    }

    on refill() {
        goto Available(max_tokens, max_tokens);
    }
}
```

Read the field name carefully: `Exhausted` is constructed with `perform now_ms()` in the `retry_after_ms` slot, so the field holds the *timestamp at which the bucket emptied*, not a duration to wait. Compute the wait yourself, or copy the source and change it.

`refill` is a single unconditional transition — the machine models the bucket, not the clock. Deciding *when* to refill is the caller's job.

## HealthCheck

Models a service's health as three states with a probe that either succeeds or moves the service down a level.

Generic over `T`, the health status payload.

| | |
| --- | --- |
| **States** | `Healthy(status)`, `Degraded(status, failures)`, `Unhealthy(reason)` |
| **Transitions** | `probe: Healthy -> Healthy \| Degraded \| Unhealthy`, `recover: Degraded -> Healthy \| Unhealthy` |
| **Effects** | `async run_probe() -> Result<T, String>` |

```gust
machine HealthCheck<T> {
    state Healthy(status: T)
    state Degraded(status: T, failures: i64)
    state Unhealthy(reason: String)

    transition probe: Healthy -> Healthy | Degraded | Unhealthy

    async effect run_probe() -> Result<T, String>

    async on probe() {
        let result = perform run_probe();
        match result {
            Ok(next_status) => {
                goto Healthy(next_status);
            }
            Err(err) => {
                goto Degraded(status, 1);
            }
        }
    }
}
```

The snippet above is trimmed to the `probe` path. The shipped source also declares `transition recover: Degraded -> Healthy | Unhealthy` with a handler that runs the same probe and goes to `Healthy` on success or `Unhealthy(err)` on failure.

Two things to know before adopting it. `probe` declares `Unhealthy` as a target but never reaches it — only `recover` does. And the failure count in `Degraded` is the literal `1` rather than an increment, so consecutive failures do not accumulate. `Err(err)` in the `probe` handler binds a name the arm never reads, which the validator warns about.

## RequestResponse

An async request lifecycle: send, then either receive a response or time out.

Generic over `T` (request) and `R` (response).

| | |
| --- | --- |
| **States** | `Pending(request, timeout_ms)`, `Completed(response)`, `Failed(error)`, `TimedOut(elapsed_ms)` |
| **Transitions** | `send: Pending -> Pending`, `receive: Pending -> Completed \| Failed`, `timeout: Pending -> TimedOut` |
| **Effects** | `async wait_for_response(request: T, timeout_ms: i64) -> Result<R, String>`, `current_time_ms() -> i64` |

```gust
machine RequestResponse<T, R> {
    state Pending(request: T, timeout_ms: i64)
    state Completed(response: R)
    state Failed(error: String)
    state TimedOut(elapsed_ms: i64)

    transition send: Pending -> Pending
    transition receive: Pending -> Completed | Failed
    transition timeout: Pending -> TimedOut

    async effect wait_for_response(request: T, timeout_ms: i64) -> Result<R, String>
    effect current_time_ms() -> i64

    on send() {
        goto Pending(request, timeout_ms);
    }

    async on receive() {
        let result = perform wait_for_response(request, timeout_ms);
        match result {
            Ok(response) => {
                goto Completed(response);
            }
            Err(err) => {
                goto Failed(err);
            }
        }
    }

    on timeout() {
        let elapsed = perform current_time_ms();
        goto TimedOut(elapsed);
    }
}
```

`send: Pending -> Pending` is a self-transition that reconstructs the state unchanged — a hook for observability rather than a state change. `timeout` stores the current timestamp in `elapsed_ms`, so, as with `RateLimiter`, the field holds a clock reading rather than a duration.

The machine has no start state other than `Pending`; you construct it with the request already in hand.

## EngineFailure

A typed failure surface for workflow-style machines. Workflow runtimes need to reason about *why* something failed — to decide retry policy, replay semantics, and what to report — without parsing strings out of an error message.

```gust
enum EngineFailure {
    UserError(String),
    SystemError(String, i64),
    IntegrationError(String, i64, String),
    Timeout(i64),
    Cancelled(String),
}
```

Import it into your own `.gu` and use it in a state field:

```gust
use std::EngineFailure;

machine Job {
    state Running
    state Failed(failure: EngineFailure)

    transition abort: Running -> Failed

    effect classify() -> EngineFailure

    on abort() {
        goto Failed(perform classify());
    }
}
```

Gust enum payloads are positional, so the meaning of each slot lives in a comment in the source rather than in the type:

| Variant | Payload |
| --- | --- |
| `UserError(reason)` | `reason: String` |
| `SystemError(reason, attempt)` | `reason: String`, `attempt: i64` — 1-based |
| `IntegrationError(service, status_code, body)` | `service: String`, `status_code: i64` — HTTP-style, `body: String` — raw response |
| `Timeout(wall_clock_ms)` | `wall_clock_ms: i64` — elapsed ms at the timeout |
| `Cancelled(requested_by)` | `requested_by: String` — opaque canceller identifier |

Wrap it to add domain variants while keeping the engine layer intact:

```gust
enum EngineFailure {
    UserError(String),
    SystemError(String, i64),
    IntegrationError(String, i64, String),
    Timeout(i64),
    Cancelled(String),
}

enum SlackFailure {
    Engine(EngineFailure),
    RateLimited(i64),
    ChannelNotFound(String),
}
```

Note that Gust has no expression form for constructing a variant with a payload — `EngineFailure::Timeout(500)` is a parse error. Build the value in an effect and return it, as the `classify` example does above. See [Grammar](grammar.md#absent-expression-forms).

## Next steps

- [Cookbook](../cookbook/index.md) — worked versions of these patterns with effect implementations
- [Grammar](grammar.md) — the forms these machines are built from
- [Known Limitations](known_limitations.md) — backend gaps that affect the generic machines above
