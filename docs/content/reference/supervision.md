---
title: "Supervision"
description: "The supervises annotation, the spawn statement, restart strategies, and exactly how much of each backend's supervision support is generated."
type: reference
---

# Supervision

Gust borrows Erlang/OTP's supervision vocabulary: a machine declares the children
it supervises and the strategy for restarting them, and starts children with
`spawn`.

::: callout warning "Read the generated-code section before designing around this"
Supervision is the least complete part of the language. The annotation is parsed,
validated, and recorded, but neither backend wires a strategy to a runtime, and
`spawn` does not construct the child machine. What you get is a declaration plus
a hook. See [What gets generated](#what-gets-generated).
:::

## The `supervises` annotation

```
machine Engine(supervises Worker(one_for_one)) { ... }
machine Coordinator(sends Events, supervises Worker(one_for_all)) { ... }
```

The strategy is optional; omitting it gives `one_for_one`. Annotations combine
with `sends` and `receives` in a single parenthesized list.

| Strategy | Restarted when one child fails |
|---|---|
| `one_for_one` | only the failed child |
| `one_for_all` | every child |
| `rest_for_one` | the failed child and every child started after it |

Given children `[A, B, C, D, E]` where `C` fails, `one_for_one` restarts `[C]`,
`one_for_all` restarts `[A, B, C, D, E]`, and `rest_for_one` restarts
`[C, D, E]`.

## `spawn`

`spawn` starts a supervised child from inside a handler. It takes the child
machine's name and zero or more arguments.

```gust
machine Worker {
    state Idle
    state Busy

    transition begin: Idle -> Busy

    on begin() {
        goto Busy;
    }
}

machine Coordinator(supervises Worker(one_for_all)) {
    state Idle
    state Running

    transition start: Idle -> Running

    on start() {
        spawn Worker();
        goto Running;
    }
}
```

The spawn target must be a machine declared in the same program; an unknown name
is a hard error with a did-you-mean suggestion. The child does not have to appear
in the machine's `supervises` annotation — the two are checked independently.

A handler containing a `spawn` gains a supervisor parameter on its generated
transition method:

| | Rust | Go |
|---|---|---|
| Parameter | `supervisor: &gust_runtime::prelude::SupervisorRuntime` | `supervisor SupervisorRuntime` |

## What gets generated {#what-gets-generated}

Be precise about what the compiler does and does not emit.

| From | Rust | Go |
|---|---|---|
| `supervises Worker(one_for_all)` | **nothing** | a package-level `var CoordinatorSupervision = []SupervisionSpec{{Child: "Worker", Strategy: OneForAll}}` |
| `spawn Worker(args)` | `supervisor.spawn_named("Worker", async move { ... })` | `supervisor.SpawnNamed("Worker", func() error { ... })` |

What Gust generates, and what it leaves to you:

- **A supervision contract.** A machine with a `supervises` clause gets a
  `{Machine}Supervision` trait (`{Machine}Children` in Go) carrying one runner
  per child, plus a table naming each child's restart strategy —
  `WORKFLOW_ENGINE_SUPERVISION` in Rust, `WorkflowEngineSupervision` in Go.
- **`spawn` constructs the child.** `spawn Worker(cfg)` builds a real
  `Worker::new(cfg)` and hands it to your runner, which the supervisor spawns
  as a named task.
- **You write the loop.** A machine is passive — its transitions are called
  from outside — so there is no generated body to run. The runner is yours, in
  the same way the effects implementation is yours. Returning `Err` marks the
  child failed, which is what a restart strategy acts on.
- **The runtime applies the strategy, your code acts on it.**
  `SupervisorRuntime::restart_scope` computes which children a strategy says to
  restart; nothing restarts automatically.

::: callout note "Changed after 0.3.0"
In 0.3.0 the `supervises` clause emitted nothing whatsoever in Rust, and
`spawn` emitted a closure that discarded its arguments and returned success
without constructing anything. The whole feature compiled and did nothing, on
every backend.
:::

Go additionally emits the shared supervision vocabulary once per file when any
machine supervises: a `SupervisionStrategy` string type with `OneForOne`,
`OneForAll`, and `RestForOne` constants, a `SupervisionSpec` struct, and a
`SupervisorRuntime` interface with a single `SpawnNamed(name string, fn func() error) error`
method that the host implements.

## The Rust runtime

The Rust side lives in `gust-runtime`, which generated code imports through
`gust_runtime::prelude::*`.

`SupervisorRuntime` manages a Tokio `JoinSet` of child tasks:

```rust
let runtime = SupervisorRuntime::with_strategy(RestartStrategy::OneForAll);
let handle = runtime.spawn_named("worker-1", async { Ok::<(), String>(()) });

if let Some(result) = runtime.join_next().await {
    match result {
        Ok(()) => { /* child completed */ }
        Err(e) => { /* child failed */ }
    }
}
```

| Item | Role |
|---|---|
| `SupervisorRuntime` | Spawns and joins children; `new()` defaults to `OneForOne` |
| `RestartStrategy` | `OneForOne` (default), `OneForAll`, `RestForOne` |
| `SupervisorRuntime::restart_scope(failed_index, count)` | The range of child indices the strategy says to restart |
| `Supervisor` | A trait you implement to decide what happens on failure |
| `SupervisorAction` | `Restart`, `Escalate`, or `Ignore` — what `Supervisor::on_child_failure` returns |

`restart_scope` computes the range; applying it — actually restarting those
children — is the host's responsibility. `gust-runtime` does not restart anything
on your behalf.

## Next

- [Channels](channels.md) — the other machine-header annotations
- [Lifecycle](lifecycle.md) — how the supervisor parameter reaches a transition
  method
- [Known Limitations](../appendix/known_limitations.md) — the scope of
  inter-machine transport
