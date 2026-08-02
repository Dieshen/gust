---
title: "Cookbook"
description: "Task-oriented recipes for the problems Gust machines get reached for, each with a complete machine, the effects the host implements, and the caveats that apply."
type: guide
---

# Cookbook

Each recipe starts from a problem, gives you a machine that solves it, and tells you what your host language still has to implement. The machines are complete — paste one into a `.gu` file, run `gust build`, implement the generated effects trait, and it works.

Six recipes mirror a machine in `gust-stdlib/`. Two — worker pool and pipeline — have no stdlib counterpart; they cover `channel` and supervision, which nothing else in the cookbook uses.

## Choosing a recipe

Pick by the problem, not by the pattern name.

| You need to | Recipe | Shape |
|---|---|---|
| Issue one call, wait for one reply, give up after a deadline | [Request / Response](./request_response.md) | `Pending → Completed / Failed / TimedOut` |
| Stop calling a dependency that is already failing, and probe before trusting it again | [Circuit breaker](./circuit_breaker.md) | `Closed → Open → HalfOpen` |
| Try a transient failure again, but not forever and not all at once | [Retry](./retry.md) | `Attempting ⇄ Waiting → Succeeded / Failed` |
| Run several steps that must all succeed, or all be undone in reverse | [Saga](./saga.md) | `Executing ⇄ Compensating → Committed / Aborted` |
| Admit only so many operations before making callers wait | [Rate limiting](./rate_limiting.md) | `Available ⇄ Exhausted` |
| Track whether a dependency is up, and notice when it recovers | [Health check](./health_check.md) | `Healthy → Degraded → Unhealthy` |
| Hand work to a pool of interchangeable consumers | [Worker pool](./worker_pool.md) | `channel` + `mpsc` |
| Move a payload through fixed, ordered stages | [Pipeline](./pipeline.md) | `channel` + supervision |

Some problems want two recipes. Retry sits *inside* a saga step, so a transient failure does not trigger a rollback. A circuit breaker sits *around* retry, so the retry budget is not spent on a dependency that is already down. Rate limiting sits in front of everything, because the point is to never make the call.

They compose by containment, not by inheritance: each machine stays separate and your host code sequences them. Gust has no notion of one machine extending another.

## Always run `gust check` first

`gust build` does not run the validator. A `.gu` that `gust check` rejects still produces output:

```bash
gust check order.gu          # error: goto target 'Nowhere' is not a declared target
gust build order.gu          # Generated order.g.rs   (exit 0)
```

The generated file will be wrong in a way that surfaces much later — usually as a confusing rustc or Go error in a file you are told never to edit. Chain the two so a failed check stops the build:

```bash
gust check order.gu && gust build order.gu -o src/generated
```

## Rough edges every recipe shares

These apply to all eight pages. None of them is a mistake in the recipe.

**The "code paths that don't end with a goto" warning is usually a false positive.** The validator's terminator analysis does not descend into `match` arms, so a handler whose every arm ends in `goto` still warns. Every stdlib machine that matches on a `Result` triggers it. Read the handler; if each arm transitions, ignore the warning.

**`goto` inside an `if` does not return.** It lowers to a state assignment, and the handler keeps running. Write `if`/`else` rather than relying on an early `goto` to end the handler — otherwise later statements execute against state you have already replaced, and in Rust you will usually see a `borrow of moved value` error for your trouble.

**Generated Rust puts redundant parentheses around a `let` bound to an arithmetic expression** (`let elapsed = (now - opened_at);`). That is a warning under `cargo check` and an error under `clippy -D warnings`. Either do not deny warnings on the module that includes generated code, or add `#![allow(unused_parens)]` at the include site. You cannot fix it from the `.gu` without making the source worse.

**You cannot construct a payload-carrying enum variant.** `Failure::Timeout(500)` does not parse — the grammar tries `qualified_path`, which has no argument list, before `fn_call`:

```text
 --> 13:37
   |
13 |         goto Failed(Failure::Timeout(500));
   |                                     ^---
   |
   = expected cmp_op, add_op, or mul_op
```

Only fieldless variants such as `Failure::Rejected` can be written inline. Return the populated variant from an effect instead. This is why the stdlib's `EngineFailure` values come out of a `perform`, and it shapes the saga and circuit-breaker recipes.

**Channels do not work on the Rust backend.** See the callout on [Worker pool](./worker_pool.md) and [Pipeline](./pipeline.md).

## Portability of these recipes

Every machine on these pages is concrete — no generic parameters — and has been compiled through both backends: `cargo check` on the Rust output and `go build` on the Go output.

The `gust-stdlib/` originals are generic (`Retry<T>`, `Saga<S>`, `RateLimiter<K>`), and generics are where the two backends diverge.

| Machine shape | Rust | Go |
|---|---|---|
| Concrete | compiles | compiles |
| Generic, and a handler carries a generic-typed state field into the next state | fails — `&T` where `T` is expected | compiles |
| Generic, and the type parameter appears in no state field at all | fails — `E0392: type parameter is never used` | compiles |
| Generic, but no handler ever reads a generic-typed field | compiles | compiles |

The first failure is a missing bound. Codegen reads a source-state field with `let request = request.clone();`, but the generated `impl` only bounds `T: Debug` — so `.clone()` on a `&T` resolves to `Clone for &T` and hands back a reference where the state expects a value.

Of the six stdlib machines, only `retry.gu` currently produces Rust that compiles, and only because its `T` arrives owned from an effect and is never carried forward from a state. The other five hit one of the two failing rows. All six produce Go that builds and vets clean.

That inverts the advice you may have read elsewhere: on the current master the stdlib machines are effectively *Go*-only. Each recipe page gives the concrete adaptation that works on both, and says what it changed.

::: callout info "Version note"
The bare source-state field reference (`index` rather than `ctx.index`) used throughout `gust-stdlib/` produced `undefined: index` in Go on the published 0.3.0, and `Result`-matching produced `undefined: Ok`. Both are fixed on master. The recipes here use the explicit `ctx.` form regardless — it reads better, and it is the form that has always worked on both backends.
:::
