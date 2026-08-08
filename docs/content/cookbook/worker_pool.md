---
title: "Worker Pool"
description: "Distribute work across interchangeable consumers with an mpsc channel — the intended design, and why it does not currently build for Rust."
type: guide
---

# Worker Pool

You have more jobs than you want to run at once, and the jobs are interchangeable. One producer hands work to a channel; several identical workers pull from it. Each job goes to exactly one worker, and back-pressure comes free from the channel's capacity.

This is the one recipe in the cookbook with no `gust-stdlib` counterpart, because it needs `channel` — the only Gust construct for moving values *between* machines rather than between states.

## The intended design

`Jobs` is a bounded `mpsc` channel: many producers, and each message delivered to exactly one consumer.

```gust
type Job { id: String, payload: String }

channel Jobs: Job (capacity: 64, mode: mpsc)

machine Dispatcher(sends Jobs) {
    state Idle
    state Dispatched(job_id: String)

    transition dispatch: Idle -> Dispatched

    on dispatch(ctx, job: Job) {
        send Jobs(job);
        goto Dispatched(job.id);
    }
}

machine Worker(receives Jobs) {
    state Waiting
    state Working(job: Job)
    state Done(job_id: String)

    transition take: Waiting -> Working
    transition finish: Working -> Done

    async effect run_job(job: Job) -> String

    on take(ctx, job: Job) {
        goto Working(job);
    }

    async on finish(ctx) {
        perform run_job(ctx.job);
        goto Done(ctx.job.id);
    }
}
```

That source passes `gust check` and generates for both backends. Only the Rust output fails to compile, for the reason in the callout above.

Four syntax details, since channels are easy to get wrong:

- **The channel declaration takes no trailing semicolon.** `channel Jobs: Job (capacity: 64, mode: mpsc)` — adding `;` is a parse error.
- **`mode` is `mpsc` or `broadcast`.** Use `mpsc` for work distribution, where each message goes to one consumer. Use `broadcast` for fan-out, where every consumer sees every message. A worker pool is always `mpsc`.
- **`send` takes exactly one argument.** Bundle multiple values into a `type`.
- **The annotation goes on the machine, in parentheses**: `machine Dispatcher(sends Jobs)`, `machine Worker(receives Jobs)`.

## What the generated code gives you

For Go, `gust build --target go` produces a channel struct plus a method on the sending machine:

```go "pool.g.go"
type JobsChannel struct { /* buffered chan Job */ }

func NewJobsChannel() *JobsChannel
func (c *JobsChannel) Send(msg Job) bool
func (c *JobsChannel) Receive() <-chan Job

func (m *Dispatcher) SendJobs(msg Job, ch *JobsChannel)
func (m *Dispatcher) Dispatch(job Job, jobsCh *JobsChannel) error
```

`Send` is non-blocking: it does a `select` with a `default` branch and returns `false` when the buffer is full. Both `SendJobs` and `Dispatch` **discard that boolean**, so a `send` into a full channel drops the job silently. If losing work is not acceptable, call `JobsChannel.Send` yourself and handle `false`, rather than relying on the `send` statement.

The receiving machine gets no channel plumbing at all: `Worker.Take(job Job)` takes a value you already pulled off `Receive()`. Ranging over the channel and calling `Take` for each message is your loop to write.

The intended Rust shape is the same: a `JobsChannel` wrapping `tokio::sync::mpsc` with `try_send` and an async `receive`, plus a `send_jobs` helper that today lands outside the `impl`.

## Sizing the pool

- **Capacity is your queue depth, and your memory bound.** 64 in-flight jobs of 1 MB each is 64 MB you did not budget for. Size it against the payload, not against the worker count.
- **Workers are separate machine instances, not a machine field.** Spawning N of them is host code; the `.gu` describes one worker's lifecycle.
- **Failed jobs need somewhere to go.** `Worker` above has no failure state. Add `state Failed(job_id: String, reason: String)` and a `Working -> Failed` target unless dropping the job silently is genuinely acceptable.
- **If ordering matters, this is the wrong recipe.** An `mpsc` pool gives you no ordering guarantee across workers. Use a [pipeline](./pipeline.md) instead.
