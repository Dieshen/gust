---
title: "Tokio Integration"
description: "Run a Gust machine inside an async Rust service: what async effects lower to, what a timeout transition actually guards, and where channel support currently stops."
type: guide
---

# Tokio Integration

Generated Rust is built for Tokio. Async effects lower to a future with a `Send` bound, `timeout` transitions are wrapped in `tokio::time::timeout`, and channels emit `tokio::sync` types directly. None of that is pluggable — there is no runtime abstraction layer, and the emitted code names `tokio::` paths.

This guide covers what each of those lowerings actually produces, so you can predict the shape of the code you have to write against.

## What your crate needs

```toml
[dependencies]
gust-runtime = "0.4"
serde = { version = "1", features = ["derive"] }
thiserror = "2"
tokio = { version = "1", features = ["full"] }
```

All four are needed. Generated code writes the direct paths `use serde::{Serialize, Deserialize};` and `#[derive(thiserror::Error)]`, and `gust-runtime`'s prelude re-exporting them does not make them resolvable as top-level crate names.

`features = ["full"]` is what the compiler's own verification harness builds generated code against, so it is the configuration actually covered by CI. Generated output names `tokio::sync::{mpsc, broadcast, Mutex}`, `tokio::time::{timeout, Duration}`, and — for `spawn` — the runtime's `SupervisorRuntime`, which uses `tokio::spawn`. You can narrow the feature list to `sync`, `time`, and `rt` yourself, but nothing in the repository verifies that combination.

::: callout info "Tokio is linked whether or not you use it"
Every generated `.g.rs` emits `use gust_runtime::prelude::*;`, and `gust-runtime` depends unconditionally on `tokio` with the `full` feature set. A crate consuming a purely synchronous, single-machine Gust contract still pulls in all of Tokio. This is a real compile-time and binary-size cost; it is not measured anywhere.
:::

The generated file itself does **not** emit `use tokio;`. Every reference is fully qualified. That import used to be emitted for machines with a channel or a timeout, and it tripped `clippy::single_component_path_imports` — a hard error for anyone building with `-D warnings`, in a file they are told never to edit. It was removed in 0.4.0.

## Async effects

Prefix the declaration with `async`. A handler that performs an async effect must itself be `async`:

```gust
machine Deployer {
    state Ready(name: String)
    state Live(url: String)

    transition deploy: Ready -> Live

    async effect push(name: String) -> String

    async on deploy(ctx: DeployCtx) {
        let url = perform push(ctx.name);
        goto Live(url);
    }
}
```

The generated trait method is **not** written as `async fn`. It is desugared to a return-position `impl Future`:

```rust
pub trait DeployerEffects {
    /// gust:effect -- replay-safe / idempotent
    fn push(&self, name: &str) -> impl ::core::future::Future<Output = String> + Send;
}
```

The `+ Send` is the point. An `async fn` in a public trait places no auto-trait bound on the returned future, so the trait alone does not promise it is `Send` — and any caller holding the machine across an `.await` inside a spawned task needs exactly that promise. Writing the bound explicitly is what makes a machine usable from `tokio::spawn`, which is the ordinary case.

**You do not have to write the desugared form.** Implementors can still write a plain `async fn`:

```rust
struct Deploy;

impl DeployerEffects for Deploy {
    async fn push(&self, name: &str) -> String {
        reqwest::get(format!("https://deploy/{name}")).await.unwrap().text().await.unwrap()
    }
}
```

That compiles as long as the future really is `Send`. If it is not — you are holding an `Rc`, a `RefCell` borrow, or a non-`Send` client across an `.await` — you get a compile error at the `impl`. The fix is to keep the non-`Send` value in a scope that ends before the first `.await`, not to remove the bound.

::: callout warning "This bound arrived in 0.3.0"
Effect traits before 0.3.0 declared `async fn`. Implementations whose future is not `Send` compiled against 0.2.x and stop compiling once you regenerate. Most implementations already satisfy the bound and need no change.
:::

Async effects lower to a direct `.await` at the call site — `effects.push(&name).await` — and the transition method becomes `pub async fn`.

## Timeout transitions

A `timeout` on a transition is a **watchdog on handler execution**. It is not a clock on the state.

```gust
machine Probe {
    state Idle
    state Healthy
    state Unhealthy

    transition check: Idle -> Healthy | Unhealthy timeout 5s

    async effect ping() -> bool

    async on check() {
        let up = perform ping();
        if up {
            goto Healthy;
        } else {
            goto Unhealthy;
        }
    }
}
```

Codegen wraps the whole handler body in `tokio::time::timeout`:

```rust
pub async fn check(&mut self, effects: &impl ProbeEffects) -> Result<(), ProbeError> {
    match &self.state {
        ProbeState::Idle => {
            let __transition_result = tokio::time::timeout(
                tokio::time::Duration::from_secs(5),
                async {
                    let up = effects.ping().await;
                    if up {
                        self.state = ProbeState::Healthy;
                    } else {
                        self.state = ProbeState::Unhealthy;
                    }
                    Ok::<(), ProbeError>(())
                },
            ).await;
            match __transition_result {
                Ok(Ok(())) => {},
                Ok(Err(err)) => return Err(err),
                Err(_) => {
                    return Err(ProbeError::Failed {
                        reason: format!("transition 'check' timed out after {:?}", tokio::time::Duration::from_secs(5)),
                    });
                },
            }
            Ok(())
        }
        // ...
    }
}
```

Four consequences that surprise people:

- **There is no timeout target state.** On expiry the machine stays exactly where it was and the caller gets `Err(ProbeError::Failed { reason })`. Declaring `-> Healthy | Unhealthy | TimedOut` gains you nothing; the timeout path never reaches `TimedOut`.
- **The error is the generic `Failed` variant**, not a dedicated timeout variant. Its `Display` is `transition failed: transition 'check' timed out after 5s`. If you need to distinguish a timeout from a handler failure, you are matching on that string.
- **The method becomes `async` even if the handler is synchronous.** A `timeout` on a transition whose handler performs nothing async still emits `pub async fn` and still requires a live Tokio runtime with the time driver. The validator does not warn about this.
- **Units are `ms`, `s`, `m`, `h`.** Minutes and hours are emitted as seconds arithmetic — `timeout 1h` becomes `Duration::from_secs(1 * 60 * 60)`.

Use `timeout` for what it is: "this operation must not hang."

::: callout warning "The Go lowering of `timeout` is not equivalent"
The Go backend wraps the handler with `context.WithTimeout` and then checks the deadline with a non-blocking `select` **after the body has already run**. It reports that the deadline passed; it does not interrupt the handler. Rust's `tokio::time::timeout` genuinely drops the future. A machine that relies on a timeout to bound a slow call behaves differently on the two backends.
:::

## Modelling a deadline while the machine sits idle

For "auto-resolve 30 minutes after acknowledgement", `timeout` is the wrong tool — nothing is executing, so there is nothing to time out. Stamp the entry time into the state and poll a self-transition:

```gust
type Alert { id: String, summary: String }

machine Incident {
    state Acknowledged(alert: Alert, acknowledged_at_ms: i64)
    state Resolved(alert: Alert)

    transition check_auto_resolve: Acknowledged -> Acknowledged | Resolved

    effect current_time_ms() -> i64

    on check_auto_resolve(ctx: CheckCtx) {
        let elapsed = perform current_time_ms() - ctx.acknowledged_at_ms;
        if elapsed >= 1800000 {
            goto Resolved(ctx.alert);
        } else {
            goto Acknowledged(ctx.alert, ctx.acknowledged_at_ms);
        }
    }
}
```

**Gust runs no background clock.** Your application drives this:

```rust
let mut ticker = tokio::time::interval(Duration::from_secs(30));
loop {
    ticker.tick().await;
    incident.check_auto_resolve(&effects)?;
    if matches!(incident.state(), IncidentState::Resolved { .. }) {
        break;
    }
}
```

That is application wiring, not something codegen provides. The upside is that the deadline check is an observable transition rather than hidden timer state.

For a delay you control explicitly — backoff, throttling — use a `Waiting` state with a `sleep_ms` effect, so the wait is a visible state rather than a blocking sleep inside a handler.

## Sharing a machine between tasks

Transition methods take `&mut self`. A machine is therefore not something several tasks can drive concurrently without synchronisation. The machine struct itself contains no `Arc`, no `Mutex`, and no interior mutability — it is a plain enum in a one-field struct — so the synchronisation is yours to add:

```rust
let machine = Arc::new(tokio::sync::Mutex::new(Deployer::new(name)));

let handle = tokio::spawn({
    let machine = Arc::clone(&machine);
    let effects = effects.clone();
    async move {
        machine.lock().await.deploy(&effects).await
    }
});
```

Use `tokio::sync::Mutex` rather than `std::sync::Mutex` here: the guard is held across the `.await` inside `deploy`. This is also where the `+ Send` bound on async effects earns its keep — without it, the spawned future would not satisfy `tokio::spawn`'s requirements.

## Channels

A `channel` declaration emits a Tokio-backed struct at module scope, and any handler that `send`s gets a sender parameter threaded into its transition method:

```gust
channel Jobs: String (capacity: 64, mode: mpsc)

machine Dispatcher {
    state Idle
    state Dispatched

    transition dispatch: Idle -> Dispatched

    on dispatch(job: String) {
        send Jobs(job);
        goto Dispatched;
    }
}
```

What comes out:

```rust
pub struct JobsChannel {
    sender: tokio::sync::mpsc::Sender<String>,
    receiver: tokio::sync::Mutex<tokio::sync::mpsc::Receiver<String>>,
}

impl JobsChannel {
    pub fn new() -> Self { /* tokio::sync::mpsc::channel(64usize) */ }
    pub fn sender(&self) -> tokio::sync::mpsc::Sender<String> { /* ... */ }
    pub fn try_send(&self, msg: String) { /* ... */ }
    pub async fn receive(&self) -> Option<String> { /* ... */ }
}

impl Dispatcher {
    pub fn dispatch(&mut self, job: String, jobs_tx: &tokio::sync::mpsc::Sender<String>)
        -> Result<(), DispatcherError>
    { /* let _ = jobs_tx.try_send(job); */ }
}
```

Details worth knowing before you design around it:

- **`mode` selects the Tokio primitive.** `mpsc` emits `tokio::sync::mpsc` and lowers `send` to `try_send`; `broadcast` emits `tokio::sync::broadcast` and lowers `send` to `send`. `broadcast` is the default when no `mode` is given. Pick `mpsc` for work distribution — each message reaches one consumer — and `broadcast` for fan-out.
- **`capacity` defaults to 1024** when omitted.
- **The send result is discarded.** Both lowerings emit `let _ = …`. A full `mpsc` channel drops the message silently; a `broadcast` with no subscribers does the same. If delivery matters, send through the sender yourself rather than via a `send` statement.
- **There is no receive statement.** `machine(receives Foo)` is an annotation the validator checks against declared channels; it generates nothing. Reading is done by calling `JobsChannel::receive()` or `subscribe()` by hand.
- **`send` takes exactly one argument**, and a channel declaration takes no trailing semicolon.

::: callout danger "Channel support is incomplete on the Rust backend"
Two defects in 0.4.0 that you will hit immediately:

**The emitted channel struct has no `Default` impl** despite a nullary `new()`, so generated code containing a channel fails `clippy::new_without_default` and therefore fails any build using `-D warnings`. This is tracked as issue #110 and the `channel` fixture is explicitly listed as unsupported for the Rust backend in the compiler's own backend test matrix.

**A `sends` annotation on the machine header emits a broken helper.** `machine Dispatcher(sends Jobs)` produces `pub fn send_jobs(&self, …)` at module scope, outside any `impl` block — which rustc rejects. Omitting the `sends` annotation and relying on the `send` statement alone avoids it, which is what the example above does.

Verify channel output against the real toolchain before you commit to it.
:::

## Verify

Generated code that parses but does not compile is the failure mode this whole workflow exists to catch. For an async machine, run all four:

```bash
gust check src/machines/deployer.gu
gust build src/machines/deployer.gu --compile
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

Because effects are a trait, the last one is cheap to make meaningful: implement a second, deterministic version of the effects trait and drive the machine against it. That is the main practical payoff of routing side effects through declarations instead of inlining them.

## Where to go next

- [Debugging](debugging.md) — what the validator's warnings are protecting you from.
- [Workflow Runtime Integration](workflow_runtime.md) — if you are building the engine rather than the application.
- [Supervision](../reference/supervision.md) — `spawn`, restart strategies, and `SupervisorRuntime`.
