---
name: gust
description: Write, compile, and integrate Gust state machines (.gu files) that generate Rust or Go. Use this whenever you encounter a .gu file, a .g.rs / .g.go generated file, a gust.toml manifest, the `gust` CLI, or the gust-runtime / gust-build / gust-stdlib crates — and also whenever someone asks to model a workflow, saga, retry policy, circuit breaker, or state machine in a project that has Gust available. Gust looks like Rust but is deliberately much smaller: no loops, no method calls, no struct literals, and a magic `ctx` parameter. Guessing the syntax from Rust intuition reliably produces code that will not parse, so consult this skill before writing any .gu.
---

# Gust

Gust compiles `.gu` state machine definitions into idiomatic Rust or Go. You declare states, the transitions between them, and the side effects a transition needs; the compiler generates a type-safe state enum, transition methods, and an effects trait you implement in the host language.

The critical thing to internalize: **Gust source looks like Rust but is a much smaller language.** It has no loops, no method calls, no struct literals, and no references. These are not oversights — Gust deliberately restricts the expression language so that a machine's behavior stays analyzable and so the same source can lower to both Rust and Go. Writing `.gu` by pattern-matching on Rust habits produces code that fails to parse. When you need computation that the expression language can't express, you declare an effect and let the host language do it (see *The effect escape hatch* below).

## Orient before writing

Check whether the project has existing `.gu` files and read one. Conventions like where generated output lands and whether the `ctx` idiom is in use are established per-project, and matching them matters more than anything in this document.

Then verify the toolchain: `gust --version`. If the binary is missing, `cargo install gust-cli --locked` (the crate is `gust-cli`; the binary it installs is named `gust`).

## The shape of a machine

```gust
type Order { id: String, total: i64 }

machine Checkout {
    state Cart(order: Order)
    state Paid(order: Order, receipt: String)
    state Failed(reason: String)

    transition pay: Cart -> Paid | Failed

    effect charge(order: Order) -> String

    on pay(ctx) {
        let receipt = perform charge(ctx.order);
        if ctx.order.total > 0 {
            goto Paid(ctx.order, receipt);
        } else {
            goto Failed("empty order");
        }
    }
}
```

Reading that in order: user types use `type` (not `struct`). Each `state` optionally carries fields. A `transition` names a source state and one or more `|`-separated targets. An `effect` declares a side effect the host implements. An `on` handler runs when the transition fires and must end each path with `goto`.

## The `ctx` parameter — read this before writing a handler

This is the single most confusing part of Gust and the most common source of broken code.

A handler needs to read the fields of the state it's transitioning *from*. There are two ways:

**Reference the field name directly.** Fields of the source state are in scope in the handler body:

```gust
state Planning(steps: Vec<String>)
transition begin: Planning -> Executing

on begin() {
    goto Executing(steps, 0);   // `steps` is Planning's field
}
```

**Or take a `ctx` parameter and go through it.** `ctx.steps` resolves to the same source-state field:

```gust
on begin(ctx) {
    goto Executing(ctx.steps, 0);
}
```

The rule that surprises people: **the ctx parameter is identified as the first handler parameter whose type is not a declared type**, and it is then *removed* from the generated method signature. `BeginCtx` above is intentionally never declared — it's a placeholder marking that parameter as the state accessor. Parameters with known types (`String`, `i64`, a declared `type`) become real arguments to the generated transition method.

```gust
on start(ctx, first_step: String) { ... }
// generates a method taking only `first_step: String`
```

Two consequences worth holding onto:

- **Both halves of this are fixed in 1.0, and the old failure is worth recognising in pre-1.0 code.** A typo in a parameter's type name used to silently make that parameter the ctx accessor and drop it from the signature, with `gust check` reporting "Check passed", because undeclared type names were legal by design. The accessor is now identified by *syntax* — the parameter with no type annotation — and an undeclared type name is a hard error. If you are reading a machine written before 1.0 and a handler argument is missing from the generated code, suspect a misspelled type.
- **Generic machines used to be substantially broken by this, and are not any more.** A machine's own type parameter was not a *declared* type, so `on put(value: T)` on `machine Box<T>` read `value` as the ctx marker and dropped it — generating `pub fn put(&mut self)` with no `value`, in Rust and Go alike. Since the accessor is identified by the *absence* of an annotation, `value: T` is simply an argument.
- **Don't declare a handler parameter with the same name as a source-state field.** The validator warns about this shadowing because the intent is ambiguous — you almost certainly meant to read the state field.

**Write the accessor with no type annotation, and name it `ctx`.**

```gust
on begin(ctx) { ... }                    // the from-state accessor
on begin(ctx, retries: i64) { ... }      // accessor plus a real argument
```

- **`on begin(ctx: BeginCtx)` is an error since 1.0**, naming the fix. Before 1.0 the accessor was identified by its type being *unrecognised*, so `BeginCtx` had to be a type declared nowhere. That made "the compiler does not know this name" load-bearing syntax — a typo in a type silently deleted a parameter, and any name the compiler might learn later would change signatures that already compiled.
- **The name still matters: call it `ctx`.** `validate_ctx_field_access` matches on the name, so `on t(ctx) { goto B(ctx.nonexistent); }` is a hard error while the same handler using `c` reports "Check passed" — a typo in a *field* name then goes uncaught to the host compiler. Only the parameter with no annotation is the accessor, so any name works structurally; only `ctx` is checked.

## What you cannot write

Every item here parses in Rust and fails in Gust. This list is the highest-value part of this skill.

| Not available | Instead |
|---|---|
| `for` / `while` / loops of any kind | Model iteration as a self-transition (`A -> A`) carrying an index, or push it into an effect |
| Method calls — `items.len()`, `s.trim()` | Declare an effect: `effect len(items: Vec<T>) -> i64` |
| Struct literals — `Order { id: x }` | Build the value in an effect and return it |
| `&` / `&mut` / dereferencing | Values are passed by value; the backend handles ownership |
| `"he said \"hi\""` — escaped quotes in strings | Use unescaped text; a `"` terminates the literal |
| `effect log(msg: String)` without a return | Return type is mandatory: `-> ()` for no result |
| `Ok(v) => v,` — expression match arms | Arms take blocks: `Ok(v) => { ... }`, and take no separating commas |
| Literal or nested patterns — `0 => {}`, `Ok(Some(x))` | Match arms bind plain identifiers or `_` only |
| `channel Events: Msg;` — trailing semicolon | Channel declarations take no semicolon |
| Named enum payloads — `Variant { a: i64 }` | Payloads are positional: `Variant(i64, String)` |
| **Constructing** a payload-carrying variant — `Failure::Timeout(500)` | Not expressible at all: `qualified_path` has no argument list and is tried before `fn_call`, so it fails to parse at the `(`. Only *fieldless* variants can be constructed (`Failure::Rejected`). Return payload variants from an effect instead — which is why `EngineFailure` values come out of `perform produce_failure(...)`. |
| Chained comparison — `a < b < c` | One comparison per expression |

Available in expressions: literals, identifiers, nested field access (`ctx.config.name`), function calls, `perform`, `Enum::Variant` paths, arithmetic (`+ - * / %`), comparison, `&& || !`, and parentheses.

Available as statements: `let`, `return`, `if`/`else`, `match`, `goto`, `perform`, `send`, `spawn`.

## What passes `gust check` but breaks a backend

`gust check` validates the Gust source. It does **not** promise the generated code compiles, and the two backends are not equivalent. These all pass validation cleanly and then fail downstream — each one verified against the real toolchain:

**The divergences that used to make this section frightening are closed.** Bare
source-state field references, `Result`-matching with `Ok`/`Err`, generic
machines, and `sends` all produced broken output on one backend or the other
before 1.0. All four now compile on both — verified by feeding the generated Go
to `go vet` and the generated Rust to `clippy -D warnings`, which is how the
compiler's own suite checks them.

The practical consequence is that **`gust-stdlib` is no longer Rust-only.**
`circuit_breaker.gu`, `retry.gu`, `saga.gu`, and `rate_limiter.gu` each hit at
least one of those divergences and all four now emit Go that vets clean. They
are the best available examples of idiomatic Gust *and* portable.

What remains:

| Construct | Behaviour |
|---|---|
| `-> Result<T, E>` where `E` is not `String` | **Go emission refuses**, with a validator error. Go lowers `Result` to `(T, error)`, so a non-`String` `E` cannot survive; `gust check` warns and passes, since the same source is valid Rust. |
| Unused `let` binding from a `perform` | Validator warns `unused binding '<name>'`; both backends lower it to a discard so the output compiles. The statement form is still clearer. |

```gust
perform log(msg);                // statement form — says "run this, ignore the result"
let ignored = perform log(msg);  // warns; compiles on both backends
```

The rule that still holds: **`gust check` is necessary, not sufficient.** It
validates Gust, not the code Gust emits. Build and compile the output for every
backend you ship, and use `clippy -D warnings` rather than plain `cargo check` —
consumers do. This is the lesson the compiler's own suite learned the hard way:
three backends were emitting output no compiler had ever seen, and two did not
compile. They were deleted in 1.0 rather than frozen into the stability promise.

## The effect escape hatch

Because the expression language is small, effects carry the weight of ordinary computation. This is idiomatic, not a hack — the stdlib's saga machine declares `effect len`, `effect get_step`, and `effect push_step` precisely because `.len()` and indexing don't exist:

```gust
effect len(steps: Vec<S>) -> i64
effect get_step(steps: Vec<S>, index: i64) -> S
effect push_step(steps: Vec<S>, step: S) -> Vec<S>
```

Reach for this whenever you catch yourself wanting a method. The cost is that each effect becomes a trait method you implement in Rust or Go, so keep them coarse enough to be worth implementing.

`perform` is an expression as well as a statement, so it composes inline:

```gust
goto Executing(steps, index + 1, perform push_step(completed, done));
```

## `effect` vs `action`

Both have identical syntax and lowering; the keyword records intent for replay-aware runtimes.

- **`effect`** — assumed idempotent and safe to replay (reading a row, computing a total).
- **`action`** — externally visible and *not* safe to replay (sending an email, posting a webhook).

Because a replay-aware runtime must checkpoint before an action, the validator enforces two rules: at most one action per code path, and the action must be the last side-effectful step before the `goto`. Both are invoked with `perform`.

## Async

Prefix the declaration with `async`: `async effect deploy(...) -> String`, `async on start(...)`. A handler that performs an async effect must itself be `async`.

## Compiling

```bash
gust check machine.gu                    # parse + validate, no output — run this first
gust build machine.gu                    # -> machine.g.rs beside the source
gust build machine.gu -o src/generated   # -o is a DIRECTORY, not a file path
gust build machine.gu --target go --package orders
gust build machine.gu --compile          # also typecheck the generated Rust
gust fmt machine.gu                      # format in place
gust diagram machine.gu                  # Mermaid state diagram
```

Targets: `rust` (default), `go`, and `ffi` (requires `--unstable-ffi`, outside the 1.0 stability promise). Only `go` requires `--package`. JSON Schema has its own subcommand, `gust schema`. The `wasm` and `nostd` targets were removed in 1.0 — compile the Rust backend's output to `wasm32` instead.

For more than a file or two, use a `gust.toml` manifest and `gust generate` — and `gust generate --check` in CI to assert the committed generated files are current. See `references/cli.md` for the manifest schema and the full flag surface.

Generated files are named `<stem>.g.rs` / `<stem>.g.go`. **Never edit them** — they're overwritten on the next build. Commit them or generate them in `build.rs`, but treat them as output.

That includes not letting *tools* edit them. `cargo fmt` follows `mod` declarations, so wiring a `.g.rs` in with `#[path] mod` lets rustfmt silently reformat it — which then breaks `gust generate --check` in CI. Prefer `include!`, which rustfmt does not follow. See `references/integrate.md`.

## Integrating generated Rust

Generated Rust is not self-contained — it derives `Serialize`/`Deserialize`, derives `thiserror::Error` for the machine's error enum, and imports `gust_runtime::prelude::*`. All three must be present or the output won't compile:

```toml
[dependencies]
gust-runtime = "0.3"
serde = { version = "1", features = ["derive"] }
thiserror = "2"
```

Then bring the file in and implement the effects trait. Each machine with effects generates a `{Machine}Effects` trait whose methods take `&self` and borrow their arguments; transition methods take `effects: &impl {Machine}Effects` and return `Result`. Codegen writes the file but does not touch your module tree — wiring it in is on you.

```rust
include!("processor.g.rs");

struct ProductionEffects;

impl EventProcessorEffects for ProductionEffects {
    fn validate_event(&self, event: &Event) -> String { /* ... */ }
}

let mut machine = EventProcessor::new();
machine.receive(event)?;              // no effects — this handler performs none
machine.validate(&effects)?;          // performs effects, so it takes them
if let EventProcessorState::Completed { result } = machine.state() { /* ... */ }
```

State fields become **named** struct-variant fields in Rust, taking their names from the `.gu` declaration. Calling a transition that isn't legal from the current state returns `Err` rather than panicking.

`references/integrate.md` covers `build.rs` integration via `gust-build`, module wiring patterns, and the known rough edges (including that `gust init` scaffolds path dependencies that only resolve inside the Gust repo).

## Verify before claiming success

`gust check` catches parse and validation errors but not everything downstream. When you've written or changed a `.gu`, run `gust check`, then build and compile the output (`--compile`, or `cargo check` in the consuming crate). Generated code that parses but doesn't compile is the failure mode this workflow exists to catch — three of Gust's own backends shipped output that no compiler had ever accepted until a test was added that actually fed it to one.

Take the validator's warnings seriously rather than filtering them out. An unused `let` binding is a Rust warning but a hard *error* in Go, so the same `.gu` that builds fine for one target breaks the other — the warning fires against the source so you hear it once, in one place.

## References

Read these as needed rather than up front:

- **`references/syntax.md`** — the complete grammar surface: every declaration and statement form, generics, channels, supervision, timeouts. Go here when you need a form not shown above.
- **`references/cli.md`** — every subcommand and flag, the `gust.toml` manifest schema, CI usage.
- **`references/integrate.md`** — wiring generated code into a consuming project: dependencies, `build.rs`, module layout, effects trait implementation, known gotchas.
- **`references/patterns.md`** — the stdlib machines (circuit breaker, retry, saga, rate limiter, health check, request/response) and how to model iteration, timeouts, and supervision.
