---
title: "Performance"
description: "What a Gust transition costs at runtime, described as mechanism rather than as numbers: what the code generator emits, where it allocates, and how the two backends differ."
type: guide
---

# Performance

::: callout warning "There are no benchmarks"
The Gust repository contains no benchmark suite, no `criterion` harness, and no measured timing of any kind. This page therefore contains no numbers. It describes what the code generator emits, so you can reason about the cost and measure your own workload if it matters.

Treat any performance figure you see attributed to Gust — including one you might infer from this page — as unverified until you measure it.
:::

## What a Rust transition does

Every transition method the Rust backend emits has the same skeleton. Here is a real one, for a handler that reads one of its from-state's two fields:

```rust
pub fn ship(&mut self, effects: &impl OrderProcessorEffects) -> Result<(), OrderProcessorError> {
    match &self.state {
        OrderProcessorState::Charged { order, payment: _payment } => {
            let order = order.clone();
            let tracking = effects.create_shipment(&order);
            self.state = OrderProcessorState::Shipped { order, tracking };
            Ok(())
        }
        _ => Err(OrderProcessorError::InvalidTransition {
            transition: "ship".to_string(),
            from: format!("{:?}", self.state),
        }),
    }
}
```

In order: a discriminant test against the current state, an owned copy of each field the handler actually reads, the handler body, and one enum-variant overwrite. That is the whole cost model.

### Transitions borrow; they do not deep-copy

The match scrutinee is `&self.state`, not `self.state.clone()`. This matters more than it looks. Until it was changed, every transition method opened by cloning the entire current state — all fields of the active variant, including every `String` and `Vec` payload — and it did so *before* the from-state check, so a transition that was going to be rejected paid the full copy anyway.

Now the arm binds references, and only the fields the handler references are made owned:

- **A field the handler reads and moves into the target state** is cloned: `let order = order.clone();`
- **A field of a `Copy` type** is dereferenced instead: `let remaining = *remaining;` — this avoids emitting a `.clone()` call on a primitive.
- **A field the handler never touches** is bound as `payment: _payment` and neither cloned nor dereferenced. It costs nothing.

The types treated as `Copy` in generated output are `i64`, `i32`, `u64`, `u32`, `f64`, `f32`, `bool`, the unit type, and any user `enum` whose variants all have empty payloads. Notably **`u8`, `i8`, `u16`, `i16`, `usize`, and `isize` are not in that set**, so a field of one of those types is emitted as a `.clone()` call. That is a trivially optimisable copy, not a heap allocation, but it is worth knowing if you were expecting the deref form.

The practical consequence: **a transition costs roughly the clone cost of the fields its handler actually reads.** A handler that reads nothing from its from-state and takes its data as arguments — the caller's value is moved in — copies nothing at all.

### There is no indirection

The generated Rust contains no `Box`, no `Box<dyn Trait>`, no `Arc`, no `Rc`, no `Mutex`, and no `RefCell`. The emitters cannot produce them.

- **The machine is one enum in a one-field struct.** `pub struct OrderProcessor { pub state: OrderProcessorState }`. Its size is the size of the largest state variant plus a discriminant, entirely inline. There is no per-state heap object.
- **Effects are statically dispatched.** Transition methods take `effects: &impl {Machine}Effects`, which is anonymous-generic and monomorphised — a direct call, not a vtable lookup.
- **`new()` does not allocate.** It writes the initial variant; if that state carries fields, they become constructor parameters and are moved in.

If you need shared mutable access to a machine across tasks, you supply the `Arc<Mutex<…>>` yourself. Nothing is added on your behalf, and nothing is paid for if you do not need it.

### The error path is the expensive path

The rejection arm is the one place a generated Rust transition does real work:

```rust
_ => Err(OrderProcessorError::InvalidTransition {
    transition: "ship".to_string(),
    from: format!("{:?}", self.state),
}),
```

That is two heap allocations, and the second one triggers a full `Debug` formatting walk of the entire current state — every `String`, every `Vec`, every nested struct. It is comfortably the most expensive thing a generated Rust machine can do.

It only runs when a transition is rejected, which in correct code is rare. But it means one thing concretely: **do not use rejected transitions as control flow in a hot path.** "Try the transition and see if it errors" is an idiom that reads fine and costs a state-sized `Debug` render every time it fails. Check `machine.state()` first.

## What a Go transition does

The Go backend uses a different representation, and the difference is not cosmetic.

```go
type OrderProcessor struct {
    State         OrderProcessorState           `json:"state"`
    PendingData   *OrderProcessorPendingData    `json:"pending_data,omitempty"`
    ValidatedData *OrderProcessorValidatedData  `json:"validated_data,omitempty"`
    ChargedData   *OrderProcessorChargedData    `json:"charged_data,omitempty"`
    // ... one per state that carries fields
}
```

The state is an `int` constant, and each data-carrying state gets its own nullable pointer field, all present on the struct simultaneously. So:

- **The Go machine is N pointers wide**, where N is the number of data-carrying states — not the width of the largest state, as in Rust.
- **Every transition into a data-carrying state heap-allocates.** The emitted code is `m.ValidatedData = &OrderProcessorValidatedData{…}`, and that composite literal escapes into a struct field. The previous state's data is then nilled by `clearStateData()` and becomes garbage. One allocation in, one object made unreachable, per transition. A transition into a fieldless state allocates nothing.
- **Effect arguments are passed by value.** The Go interface is `CalculateTotal(order Order) Money`, where the Rust trait is `fn calculate_total(&self, order: &Order) -> Money`. A Go effect call copies its struct arguments; the Rust one does not. Go's copies are shallow — slice, map, and string headers are copied, backing arrays shared.
- **The Go error path is cheaper than Rust's.** `&XError{Transition: "ship", From: m.State.String()}` uses a compile-time constant state name from a `switch`, with no reflection over state data. Formatting is deferred to `Error()` and only happens if the caller formats it.

If you are running the same contract on both backends and expecting comparable allocation behaviour, you will not get it. Rust keeps state inline and allocates only what the handler clones; Go allocates one state-data object per transition.

## Timeouts pull in the runtime

A `timeout` on a transition makes the generated Rust method `async` **even when the handler is entirely synchronous**, and wraps the body in `tokio::time::timeout`. That is not a micro-cost — it means a synchronous state machine now requires a live Tokio runtime with the time driver to be driven at all.

The validator does not warn about this. If you added `timeout 30s` for safety on a transition that never blocks, you have converted a plain function call into a polled future for no benefit.

## Compile-time costs

Two things dominate, and neither is runtime cost.

**Tokio is always linked.** Every generated `.g.rs` emits `use gust_runtime::prelude::*;`, and `gust-runtime` depends unconditionally on `tokio` with the `full` feature set. A crate consuming a purely synchronous Gust contract still builds and links all of Tokio. The dependency is confirmed; its size and build-time cost are not measured anywhere.

**`gust-build` pulls the whole compiler.** Using the build-script helper means adding `gust-lang` to `[build-dependencies]`, which brings `pest`, the `pest_derive` proc macro, `thiserror`, `strsim`, `colored`, and `serde_json` — including the `syn` / `quote` / `proc-macro2` chain, which must be built for the *host* even when cross-compiling. That is a real cost imposed on everyone who builds your crate.

The alternative is to commit the generated files and drop the build dependency entirely. Neither choice is wrong:

| | Generate in `build.rs` | Commit the output |
| --- | --- | --- |
| Staleness | Impossible | Needs `gust generate --check` in CI |
| Build dependency | The whole compiler | None |
| Reviewability | Output invisible in PRs | Codegen changes show up in diffs |

If compile time matters to your consumers, commit the output.

**`gust-build` skips up-to-date files** by comparing modification times, which makes incremental rebuilds nearly free — and which has a sharp edge: after a fresh clone, every file shares one checkout timestamp, so a genuinely stale generated file is never rewritten. That is a correctness problem wearing a performance optimisation's clothes. Guard committed output with `gust generate --check`.

## If you need to measure

Nothing here is a substitute for measuring your own workload, and the mechanism above tells you where to look first:

1. **Profile the effects, not the transitions.** Effect implementations are ordinary host code doing the actual I/O and computation. The transition scaffolding around them is a discriminant test and some field clones.
2. **Look at what your states carry.** A state holding a large `Vec` that several handlers read is cloned on each of those transitions. Grouping configuration into a single `type` carried by reference-free value does not avoid the clone — reducing what handlers *read* does.
3. **Count rejected transitions.** If your call pattern produces them routinely, the `format!("{:?}", …)` in the error arm is worth eliminating by checking state first.
4. **On Go, count transitions into data-carrying states.** Each one is an allocation.

## Where to go next

- [Debugging](debugging.md) — if the problem turns out to be correctness rather than speed.
- [Tokio Integration](tokio_integration.md) — the async lowerings and what they require at runtime.
- [Code Generation](../advanced/codegen.md) — the emitters themselves.
