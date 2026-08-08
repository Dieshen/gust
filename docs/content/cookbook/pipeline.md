---
title: "Pipeline"
description: "Move a payload through ordered stages under a supervisor, with the channel caveat and what `spawn` actually generates."
type: guide
---

# Pipeline

A batch arrives, gets parsed, gets enriched, gets written. Each stage does one thing, each stage can fail independently, and a failure in stage three should not lose the work stages one and two already did.

A pipeline differs from a [worker pool](./worker_pool.md) in one respect: the stages are *not* interchangeable. Order matters, so each stage is its own machine with its own state, and a supervisor decides what happens when one of them dies.

## The machine

`Pipeline` supervises `Stage`. Each stage transforms a batch and reports back; the supervisor tracks progress and decides when the pipeline has drained.

```gust
type Batch { id: String, size: i64 }

machine Stage {
    state Idle
    state Transforming(batch: Batch)
    state Emitted(batch_id: String)

    transition accept: Idle -> Transforming
    transition emit: Transforming -> Emitted

    effect transform(batch: Batch) -> Batch

    on accept(ctx, batch: Batch) {
        goto Transforming(perform transform(batch));
    }

    on emit(ctx) {
        goto Emitted(ctx.batch.id);
    }
}

machine Pipeline(supervises Stage(one_for_one)) {
    state Stopped
    state Running(stages: i64, processed: i64)
    state Drained(processed: i64)

    transition start: Stopped -> Running
    transition advance: Running -> Running | Drained

    effect remaining(processed: i64) -> i64

    on start(ctx, first: Batch) {
        spawn Stage(first);
        goto Running(1, 0);
    }

    on advance(ctx) {
        let left = perform remaining(ctx.processed);
        if left > 0 {
            goto Running(ctx.stages, ctx.processed + 1);
        } else {
            goto Drained(ctx.processed);
        }
    }
}
```

Save as `pipeline.gu` and build. This one compiles cleanly for both backends — it has no channel annotations.

Note that `Stage.accept` takes `batch: Batch` as a real argument while `ctx: AcceptCtx` is dropped from the generated signature. The `ctx` parameter is identified as the first handler parameter whose type is not a declared type; `Batch` *is* declared, so it stays.

## Restart strategies

The strategy in `supervises Stage(one_for_one)` decides what dies with a failed child.

| Strategy | Restarts | Use when |
|---|---|---|
| `one_for_one` | only the failed child | Stages hold no shared state — a restarted stage can pick up from the queue |
| `one_for_all` | every child | Stages share state that a partial restart would leave inconsistent |
| `rest_for_one` | the failed child and everything started after it | Downstream stages depend on the failed one, upstream stages do not |

A pipeline whose stages communicate only through queues wants `one_for_one`. A pipeline whose stages share a transaction wants `one_for_all`.

## What `spawn` actually generates

This is the part to check before designing around it. `spawn Stage(first)` lowers to:

```rust "pipeline.g.rs"
pub fn start(
    &mut self,
    first: Batch,
    supervisor: &gust_runtime::prelude::SupervisorRuntime,
) -> Result<(), PipelineError> {
    // ...
    let _child = supervisor.spawn_named("Stage", async move { let _ = (first); Ok::<(), String>(()) });
    // ...
}
```

Codegen registers a **named child task with an empty body**. It does not construct a `Stage`, does not drive its transitions, and discards the arguments. The supervisor bookkeeping is real — `SupervisorRuntime` tracks children, holds a `RestartStrategy`, and computes which children a failure should take down — but the child's actual work is yours to supply.

So `supervises` and `spawn` today buy you a declared topology and a restart policy, not a running child. Plan on wiring the stage loop in host code and using the supervisor for what it does provide.

## What the host implements

Two effects, both narrow.

```rust "src/pipeline_effects.rs"
impl StageEffects for Enricher {
    fn transform(&self, batch: &Batch) -> Batch {
        Batch { id: batch.id.clone(), size: batch.size * 2 }
    }
}

impl PipelineEffects for Coordinator {
    fn remaining(&self, processed: i64) -> i64 {
        self.total.saturating_sub(processed)
    }
}
```

`remaining` is the machine asking the host a question it cannot answer itself — how much work is left. That is the right division: the host knows the queue depth, the machine knows what to do about it.

## The intended channel wiring

If you want stages joined by channels rather than by host code, this is the shape — and the shape that does not build for Rust today:

```gust
type Record { id: String, body: String }

channel Parsed: Record (capacity: 128, mode: mpsc)

machine Parser(sends Parsed) {
    state Reading
    state Sent(record_id: String)

    transition parse: Reading -> Sent

    on parse(ctx, record: Record) {
        send Parsed(record);
        goto Sent(record.id);
    }
}

machine Enricher(receives Parsed) {
    state Waiting
    state Enriched(record: Record)

    transition accept: Waiting -> Enriched

    effect enrich(record: Record) -> Record

    on accept(ctx, record: Record) {
        goto Enriched(perform enrich(record));
    }
}
```

One channel per stage boundary, `mpsc` throughout because each record should be handled once. Use `broadcast` only where every downstream stage genuinely needs every record — a metrics tap alongside the main path, for instance.

## Design notes

- **Channel capacity is the buffer between stages.** Set it from how bursty the upstream is and how much memory a queued payload costs, not from a round number.
- **A slow stage should push back, not drop.** The generated `Send` returns `false` on a full buffer and the `send` statement discards it. If you need back-pressure rather than silent loss, call the channel's send method from host code and handle the `false`.
- **Give every stage a failure state.** `Stage` above cannot fail, which is only realistic for a pure transform. Anything touching a network needs `state Failed(batch_id: String, reason: String)` and a transition that reaches it.
- **Keep stages single-purpose.** The reason to build a pipeline instead of one machine is that each stage restarts, scales, and fails on its own. A stage doing three things gives up all three.
