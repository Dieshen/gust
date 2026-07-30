---
title: "Migrating from Rust"
description: "Move a hand-written Rust state machine into a .gu contract by deciding what becomes a state, what becomes a transition, and what gets pushed down into an effect."
type: guide
---

# Migrating from Rust

You have a state machine written by hand in Rust — an enum with a `match`, or a struct with a `status` field and a pile of methods that mutate it. You want it expressed as a `.gu` contract so the state graph is checkable, diagrammable, and shared with a Go service.

The mechanical part of that migration is easy. The part that decides whether it is worth doing is knowing what Gust will refuse to accept.

## Read this before you start

**Gust is a much smaller language than Rust.** It has no loops, no method calls, no struct literals, no references, and no closures. That is deliberate: the expression language is restricted so the machine's behaviour stays analysable and so the same source can lower to both Rust and Go.

The practical consequence is that migration is not a translation. It is a **separation**. Your existing code splits into two piles:

- **The state graph** — which states exist, what data each carries, which transitions are legal, and which branch is taken. This goes into the `.gu`.
- **Everything else** — parsing, arithmetic beyond the basics, collection handling, I/O, formatting, anything that calls a method. This stays in Rust, behind an `effect` declaration.

If your machine is 90% computation and 10% state graph, you will end up with a `.gu` that is mostly effect declarations and a Rust file that looks much like the one you started with. That is a signal that Gust is the wrong tool for that component, not a signal that you have migrated it badly.

Gust earns its keep when the state graph is the interesting part: an order lifecycle, a saga, an approval workflow, a retry policy. Those are the ones where "which transitions are legal" is a question people argue about, and where having it written down and enforced pays for the restrictions.

## The three-column translation

Work through your existing code in this order.

### 1. Your enum becomes states

A Rust enum variant with data becomes a `state` with fields. Field names carry over — the Rust backend emits named struct-variant fields taken straight from the `.gu` declaration.

```rust
enum Upload {
    Idle,
    Uploading { path: String, attempt: u32 },
    Done { url: String },
    Failed { reason: String },
}
```

Two adjustments. Gust's integer type is `i64`, so `u32` becomes `i64`. And a state that encodes failure in a boolean field should become its own terminal state carrying a reason — it makes the state graph and the generated diagram honest.

### 2. Your `match` arms become transitions

Every arm of your dispatch `match` that changes the state is a transition. Name it for the event, in snake_case, and declare every state it can reach:

```
transition attempt: Uploading -> Uploading | Done | Failed
```

The `|`-separated target list is enforced. A `goto` to a state that is not in the list is a validation error, which is most of the value you are buying.

### 3. Everything else becomes an effect

This is the step people underestimate. Each of these is a compile error in Gust, and the fix is always the same shape:

| In your Rust | In Gust |
| --- | --- |
| `items.len()` | `effect len(items: Vec<String>) -> i64` |
| `items[i]` | `effect item_at(items: Vec<String>, index: i64) -> String` |
| `s.trim().to_lowercase()` | `effect normalise(s: String) -> String` |
| `Receipt { id, total }` | `effect make_receipt(total: i64) -> Receipt` |
| `client.put(&path).await?` | `async effect put_object(path: String) -> String` |
| `for item in items { … }` | a self-transition (see below) |

An effect declaration becomes one method on a generated trait that you implement in Rust. The body you move there is the body you already had.

Two judgement calls when you draw the line:

- **Keep effects coarse.** Every effect is a trait method somebody implements — in Rust *and*, if you target it, in Go. One `compute_backoff` beats four effects for multiply, min, random, and clamp.
- **Push logic down, not control flow up.** An effect that computes a value is right. An effect that decides which state to go to next has moved the machine's logic into the host language, which defeats the point of migrating.

Return types are mandatory. An effect with no meaningful result is declared `-> ()`.

## A worked example

The Rust you are starting from — an upload with a bounded retry:

```rust
impl Upload {
    fn attempt(&mut self, client: &S3) {
        loop {
            match client.put_object(&self.path) {
                Ok(url) => {
                    *self = Upload::Done { url };
                    return;
                }
                Err(_) if self.attempt < MAX_ATTEMPTS => {
                    self.attempt += 1;
                }
                Err(e) => {
                    *self = Upload::Failed { reason: e.to_string() };
                    return;
                }
            }
        }
    }
}
```

The same machine in Gust. The `loop` is gone — each attempt is now a transition the caller fires, so every attempt is an observable state:

```gust
machine Upload {
    state Idle
    state Uploading(path: String, attempt: i64)
    state Done(url: String)
    state Failed(reason: String)

    transition begin: Idle -> Uploading
    transition attempt: Uploading -> Uploading | Done | Failed

    effect put_object(path: String) -> String
    effect max_attempts() -> i64

    on begin(path: String) {
        goto Uploading(path, 1);
    }

    on attempt(ctx: AttemptCtx) {
        let url = perform put_object(ctx.path);
        if url != "" {
            goto Done(url);
        } else if ctx.attempt < perform max_attempts() {
            goto Uploading(ctx.path, ctx.attempt + 1);
        } else {
            goto Failed("upload failed after retries");
        }
    }
}
```

Then the code you removed reappears as the effects implementation, which is where it belonged all along:

```rust
struct S3Effects { client: S3 }

impl UploadEffects for S3Effects {
    fn put_object(&self, path: &str) -> String {
        self.client.put_object(path).unwrap_or_default()
    }

    fn max_attempts(&self) -> i64 {
        5
    }
}
```

Note what the migration bought and what it cost. Bought: the retry ceiling and the failure path are now in the contract, the state graph renders as a diagram, and a Go service can be generated from the same source. Cost: the caller now drives the loop by calling `attempt()` until the machine lands somewhere terminal.

## The `ctx` parameter

Handlers read the fields of the state they are transitioning *from*. Take a `ctx` parameter and go through it — `ctx.path` resolves to the `Uploading` state's `path` field.

`AttemptCtx` above is never declared anywhere, and that is intentional. **The ctx parameter is identified as the first handler parameter whose type is not a declared type**, and it is then removed from the generated method signature. Parameters with known types become real arguments.

This has a sharp edge worth knowing during a migration, when you are typing a lot of new type names:

::: callout warning "A typo in a parameter's type silently deletes the parameter"
Because undeclared type names are legal by design, `on pay(odrer: Order)` reads `odrer` as the ctx accessor and drops it from the generated method. `gust check` reports "Check passed" — it cannot catch this. If a handler argument mysteriously vanishes from the generated code, suspect a misspelled type on the first parameter.
:::

Fields of the source state are also in scope by bare name, without `ctx.`. Prefer the explicit `ctx.` form anyway: it reads better, and it is what the Go backend expects. See [Debugging](debugging.md#when-check-passes-and-the-backend-does-not) for the portability details.

## Migrating a loop

There is no `for` and no `while`. Model iteration as a transition from a state back to itself, carrying the cursor in a state field:

```gust
machine Batch {
    state Ready(items: Vec<String>)
    state Processing(items: Vec<String>, index: i64, done: i64)
    state Complete(done: i64)

    transition start: Ready -> Processing
    transition step: Processing -> Processing | Complete

    effect len(items: Vec<String>) -> i64
    effect item_at(items: Vec<String>, index: i64) -> String
    effect handle(item: String) -> bool

    on start(ctx: StartCtx) {
        goto Processing(ctx.items, 0, 0);
    }

    on step(ctx: StepCtx) {
        if ctx.index >= perform len(ctx.items) {
            goto Complete(ctx.done);
        }
        let item = perform item_at(ctx.items, ctx.index);
        let handled = perform handle(item);
        if handled {
            goto Processing(ctx.items, ctx.index + 1, ctx.done + 1);
        } else {
            goto Processing(ctx.items, ctx.index + 1, ctx.done);
        }
    }
}
```

The loop body is the handler; the caller fires `step()` until the machine reaches `Complete`. This is more verbose than a `for`, and that is the trade being made deliberately: every iteration is an observable state, which is what makes the machine replayable and inspectable.

A loop with distinct phases uses a cycle of states rather than one. A retry that waits between attempts alternates `Attempting -> Waiting -> Attempting`, so the delay is its own state rather than a blocking sleep hidden inside a handler.

## Migrating a struct literal

You cannot build a value inline. Declare the type, and build it in an effect:

```gust
type Receipt { id: String, total: i64 }

machine Checkout {
    state Cart(total: i64)
    state Paid(receipt: Receipt)

    transition pay: Cart -> Paid

    effect make_receipt(total: i64) -> Receipt

    on pay(ctx: PayCtx) {
        let receipt = perform make_receipt(ctx.total);
        goto Paid(receipt);
    }
}
```

Note `type`, not `struct`. Enum variant payloads are positional — `Failed(String, i64)`, never `Failed { reason: String }`.

## Migrating `Result` and `?`

There is no `?` operator, and `Result` needs care if Go is in your future.

If Rust is your only target, `Result` plus `match` works and reads naturally:

```gust
machine Deployer {
    state Ready(name: String)
    state Live(url: String)
    state Broken(reason: String)

    transition deploy: Ready -> Live | Broken

    async effect push(name: String) -> Result<String, String>

    async on deploy(ctx: DeployCtx) {
        let outcome = perform push(ctx.name);
        match outcome {
            Ok(url) => {
                goto Live(url);
            }
            Err(err) => {
                goto Broken(err);
            }
        }
    }
}
```

That machine draws a `handler 'deploy' has code paths that don't end with a goto` warning even though every arm ends in `goto`. It is a known false positive — the termination analysis does not descend into match arms. See [Debugging](debugging.md#the-false-positive-worth-knowing-about).

As of 0.4.0 this lowers to Go correctly as well: an effect declared `-> Result<T, E>` becomes Go's `(T, error)` idiom, and the `Ok`/`Err` match becomes a nil check on the error. Before 0.4.0 the emitted Go did not compile — the match had nothing left to match on.

One constraint survives, and the validator now warns about it: **Go signals failure with a single `error`, so `E` is erased.** `Result<T, String>` round-trips, because the `Err` binding receives `err.Error()`. Any other `E` leaves the `Err` binding holding a Go `error`, which will not typecheck where `E` was expected. If a machine must target both backends, declare fallible effects as `Result<T, String>`.

Match arms take blocks and no separating commas — `Ok(url) => { … }`, not `Ok(url) => …,`. Arms bind plain identifiers or `_`; there are no literal or nested patterns.

## What to check when you are done

Run these in order. Each catches something the previous one cannot.

```bash
gust check src/machines/upload.gu      # parse + validate the source
gust build src/machines/upload.gu --compile   # generate Rust and typecheck it
cargo clippy --all-targets -- -D warnings     # what your consumers will run
```

::: callout warning "`gust build` does not validate"
`gust build` parses and generates. It does **not** run the validator, and it will happily emit code for a source with undefined states and undeclared effects. Only `gust check` (and `gust schema`) validate. Run `gust check` first, every time.
:::

If Go is also a target, add:

```bash
gust build src/machines/upload.gu --target go --package upload -o ./go
cd go && go vet ./...
```

A short migration checklist to run down before you call it finished:

- Every handler path ends in a `goto`.
- Every `goto` target appears in that transition's declared target list.
- No handler parameter shares a name with a field of its from-state — the field wins, and the parameter becomes dead.
- Fallible effects are `Result<T, String>` if Go is a target, or a plain `bool`/sentinel flag.
- The generated code compiles under `clippy -D warnings`, not just `cargo check`. Consumers use the former.

## Where to go next

- [Debugging](debugging.md) — reading the validator's output, and the cases where `gust check` passes but a backend does not.
- [Tokio Integration](tokio_integration.md) — if the effects you just extracted are async.
- [Cookbook](../cookbook/) — the shapes you are most likely reaching for, already written down.
