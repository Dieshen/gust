---
title: "Effects and Handlers"
description: "Declaring effects and actions, writing on handlers, the ctx parameter rule, and how perform lowers into Rust and Go."
type: reference
---

# Effects and Handlers

Effects, actions, and handlers are the boundary between a Gust machine and the
host application that runs generated code.

Use this page as the language reference. For replay and checkpointing design,
see [Workflow Runtime Integration](../guides/workflow_runtime.md).

## Effects

An `effect` declares an operation that the generated machine can call through
the host application's effects interface.

```gust
machine OrderChecks {
    state Pending(order_id: String)
    state Accepted(order_id: String, token: String)
    state Rejected(order_id: String, reason: String)

    transition validate: Pending -> Accepted | Rejected

    effect validate_order(order_id: String) -> Result<String, String>

    on validate(ctx: ValidateCtx) {
        let result = perform validate_order(ctx.order_id);
        match result {
            Ok(token) => {
                goto Accepted(ctx.order_id, token);
            }
            Err(reason) => {
                goto Rejected(ctx.order_id, reason);
            }
        }
    }
}
```

Effects are assumed to be replay-safe or idempotent. Typical examples are
calculating a value, reading configuration, validating an input, or querying a
service where repeated calls are acceptable for the surrounding runtime.

The return type is mandatory. Write `-> ()` when an effect produces no value;
`effect log(msg: String)` is a parse error.

Effects can be asynchronous:

```gust
machine AsyncCheck {
    state Pending(id: String)
    state Done(token: String)

    transition validate: Pending -> Done

    async effect fetch_token(id: String) -> String

    async on validate(ctx: ValidateCtx) {
        let token = perform fetch_token(ctx.id);
        goto Done(token);
    }
}
```

A handler that performs an `async` effect must itself be `async`.

An effect cannot declare its own type parameters, but it may use the machine's:
`effect get_step(steps: Vec<S>, index: i64) -> S` is valid inside
`machine Saga<S>`.

## Actions

An `action` has the same signature shape as an `effect`, but it marks an
operation as non-idempotent or externally visible.

```gust
machine Approval {
    state Waiting(step: String)
    state Rejected(step: String, reason: String)

    transition reject: Waiting -> Rejected

    effect normalize_reason(reason: String) -> String
    action notify_rejection(step: String, reason: String) -> String

    on reject(ctx: RejectCtx, reason: String) {
        let cleaned = perform normalize_reason(reason);
        let receipt = perform notify_rejection(ctx.step, cleaned);
        goto Rejected(ctx.step, receipt);
    }
}
```

Use `action` for work such as charging a card, sending an email, publishing a
webhook, or recording an externally visible decision. Replay-aware runtimes can
use the action marker to checkpoint before execution and restore the recorded
result during replay instead of running the operation again.

The validator emits warnings for two unsafe handler shapes:

- More than one `action` on a single code path.
- Any side-effectful step after an `action` on the same path.

Branches are analyzed independently, so different actions in sibling `if` or
`match` branches are allowed when only one branch can run.

## Handlers

A handler is an `on <transition>(...) { ... }` block. It defines the code that
runs when the host application invokes a generated transition method.

```gust
machine Shipment {
    state Charged(order_id: String)
    state Shipped(order_id: String, tracking: String)
    state Failed(order_id: String, reason: String)

    transition ship: Charged -> Shipped | Failed

    effect create_shipment(order_id: String) -> Result<String, String>

    on ship(ctx: ShipCtx) {
        let result = perform create_shipment(ctx.order_id);
        match result {
            Ok(tracking) => {
                goto Shipped(ctx.order_id, tracking);
            }
            Err(reason) => {
                goto Failed(ctx.order_id, reason);
            }
        }
    }
}
```

The handler name must match a declared transition name. Every reachable branch
should terminate with `goto <State>(...)`, and — because
[`goto` does not return](states_transitions.md#goto-does-not-return) — with
exactly one.

Two forms the grammar accepts and the validator rejects:

- A handler return type. `on compute() -> i64 { ... }` parses, then fails with
  `handler return types are not yet supported`.
- A `return` statement. `gust check` reports `return statements are not supported
  in handlers; use goto to transition`.

The validator also warns when a declared transition has no handler and when
handler branches can fall through without transitioning.

## The `ctx` parameter {#the-ctx-parameter}

This is the most confusing part of Gust and the most common source of broken
generated code.

A handler frequently needs to read fields of the state it is transitioning
*from*. The mechanism is a parameter whose type is a placeholder:

```gust
machine Planner {
    state Planning(steps: Vec<String>, owner: String)
    state Executing(steps: Vec<String>, index: i64)

    transition begin: Planning -> Executing

    on begin(ctx: BeginCtx) {
        goto Executing(ctx.steps, 0);
    }
}
```

`BeginCtx` is deliberately never declared. `ctx.steps` resolves to the source
state's `steps` field, and the parameter is **removed from the generated method
signature** — `Planner::begin()` in Rust takes no arguments beyond `&mut self`.

### The detection rule

The `ctx` parameter is the **first handler parameter whose type is not a known
type**. Known types are:

- every `type` and `enum` declared in the program,
- the builtins `String`, `i64`, `i32`, `u64`, `u32`, `f64`, `f32`, `bool`, `Vec`,
  `Option`, `Result`,
- the machine's own generic parameters.

Only a *simple* type name can mark a `ctx` parameter. A generic type expression
such as `Vec<Thing>` never qualifies, however unfamiliar `Thing` is.

Parameters with known types become real arguments:

```gust
machine Starter {
    state Idle
    state Running(first_step: String)

    transition start: Idle -> Running

    on start(ctx: StartCtx, first_step: String) {
        goto Running(first_step);
    }
}
```

That generates `pub fn start(&mut self, first_step: String)` in Rust and
`func (m *Starter) Start(first_step string) error` in Go.

If no parameter qualifies but the body mentions `ctx`, the name `ctx` is used
implicitly.

### Three consequences

::: callout warning "A typo in a type name silently deletes a parameter"
Undeclared type names are legal by design — that is what makes `BeginCtx` work.
So `on pay(order: Ordr)` makes `order` the ctx accessor and drops it from the
generated signature. `gust check` reports "Check passed"; it cannot tell the two
cases apart. If an argument mysteriously vanishes from the generated code,
suspect a misspelled type on the first parameter.
:::

**Name the parameter `ctx`.** The detection rule keys off the type, but the
validator's field-availability check keys off the literal name. With
`on finish(ctx: FinishCtx)`, referencing a field the source state does not have
is a hard error:

```
error: field 'nonexistent' not available in state 'Pending'
   = note: available fields: order
```

With `on finish(c: FinishCtx)` the same code passes validation and fails later
in the host language. The generated code is identical; only the diagnostic is
lost.

**Do not name a value parameter after a source-state field.** Codegen
destructures the source state inside the transition method, so that binding
shadows the parameter and makes it unreachable:

```
warning: handler parameter 'step' is shadowed by the from-state field of the same name
   = help: rename the parameter, or drop it and read 'step' from the state
```

### Bare field references

Source-state fields are also in scope by bare name, without a `ctx` parameter:

```gust
machine Bucket {
    state Available(tokens: i64, max_tokens: i64)
    state Exhausted(max_tokens: i64)

    transition acquire: Available -> Available | Exhausted

    on acquire() {
        if tokens > 0 {
            goto Available(tokens - 1, max_tokens);
        } else {
            goto Exhausted(max_tokens);
        }
    }
}
```

Both backends support this form as of 0.4.0 — the Rust backend gets the names
from its destructured match arm, and the Go backend lifts them into locals. In
the published 0.3.0 the Go output failed with `undefined: tokens`, which is why
`gust-stdlib`, written entirely in this style, was effectively Rust-only.

Prefer the explicit `ctx.` form anyway. It says which names come from the state
and which are local, and it is the form the validator checks.

## `perform`

Use `perform` to call either an effect or an action:

```gust
machine Audit {
    state Ready
    state Done(record_id: String)

    transition record: Ready -> Done

    effect build_payload() -> String
    action write_audit(payload: String) -> String

    on record() {
        let payload = perform build_payload();
        let record_id = perform write_audit(payload);
        goto Done(record_id);
    }
}
```

`perform` is an expression as well as a statement, so it composes inline:

```gust
machine Walker {
    state Executing(steps: Vec<String>, index: i64, done: Vec<String>)
    state Finished(done: Vec<String>)

    transition advance: Executing -> Executing | Finished

    effect len(steps: Vec<String>) -> i64
    effect get(steps: Vec<String>, index: i64) -> String
    effect push(done: Vec<String>, step: String) -> Vec<String>

    on advance(ctx: AdvanceCtx) {
        if ctx.index >= perform len(ctx.steps) {
            goto Finished(ctx.done);
        } else {
            let step = perform get(ctx.steps, ctx.index);
            goto Executing(ctx.steps, ctx.index + 1, perform push(ctx.done, step));
        }
    }
}
```

The argument count is checked against the declaration
(`effect 'x' expects N argument(s) but got M`), and a `let` with an explicit
annotation is checked against the effect's declared return type.

### Discarding a result

When you do not need an effect's value, use the statement form:

```
perform log(msg);                // statement form — always correct
let ignored = perform log(msg);  // warns: unused binding 'ignored'
```

The binding form emits:

```
warning: unused binding 'ignored'
   = note: the value is never read; Go codegen rejects unused locals outright
   = help: remove the binding, or call the effect without binding it
```

There is no underscore-prefix exemption: `_ignored` warns too, because Go accepts
only a bare `_` and an exempted binding would reach the backend the diagnostic
exists to protect. Both backends lower an unread binding to a discard (`let _ =`
and `_ =`), so the effect still runs and the output compiles.

## The effect escape hatch {#the-effect-escape-hatch}

Because the [expression language is small](syntax.md#what-you-cannot-write),
effects carry the weight of ordinary computation. This is idiomatic, not a
workaround. The standard library's saga machine declares `len`, `get_step`, and
`push_step` precisely because `.len()` and indexing do not exist:

```
effect len(steps: Vec<S>) -> i64
effect get_step(steps: Vec<S>, index: i64) -> S
effect push_step(steps: Vec<S>, step: S) -> Vec<S>
```

Reach for this whenever you catch yourself wanting a method call, a struct
literal, an index, or a cast. The cost is that each effect becomes a trait or
interface method the host must implement, so keep them coarse enough to be worth
implementing.

## Code Generation

Generated Rust and Go code exposes one effects interface per machine. Both
`effect` and `action` declarations become methods on that interface. Each method
is marked in generated comments so host runtimes can preserve the semantic
distinction.

For example, a Rust target generates a trait whose methods are implemented by
the host application. Each Rust trait method gets a machine-readable doc comment
directly above it:

```rust
pub trait AuditEffects {
    /// gust:effect -- replay-safe / idempotent
    fn build_payload(&self) -> String;
    /// gust:action -- not replay-safe / externally visible
    fn write_audit(&self, payload: &str) -> String;
}
```

A Go target generates an interface with the same role and marker format:

```go
type AuditEffects interface {
    // gust:effect -- replay-safe / idempotent
    BuildPayload() string
    // gust:action -- not replay-safe / externally visible
    WriteAudit(payload string) string
}
```

The generated transition method receives that implementation and calls the
declared methods while executing the handler body.

### Signature lowering {#signature-lowering}

Every combination of `async` and return type produces a different method shape.

| Gust declaration | Rust trait method | Go interface method |
|---|---|---|
| `effect f(x: String) -> String` | `fn f(&self, x: &str) -> String` | `F(x string) string` |
| `effect f(x: String) -> ()` | `fn f(&self, x: &str)` | `F(x string)` |
| `effect f(x: String) -> Result<String, String>` | `fn f(&self, x: &str) -> Result<String, String>` | `F(x string) (string, error)` |
| `async effect f(x: String) -> String` | `fn f(&self, x: &str) -> impl Future<Output = String> + Send` | `F(ctx context.Context, x string) (string, error)` |
| `async effect f(x: String) -> ()` | `fn f(&self, x: &str) -> impl Future<Output = ()> + Send` | `F(ctx context.Context, x string) error` |

Three details worth internalizing:

- **Rust borrows non-`Copy` parameters.** `String` becomes `&str`, `Vec<T>`
  becomes `&[T]`, a declared `type` becomes `&Type`. Numeric types and `bool` are
  passed by value. Go passes everything by value.
- **Rust desugars `async` to `-> impl Future + Send`** rather than writing
  `async fn`, which would trip the `async_fn_in_trait` lint. Implementors can
  still write a plain `async fn`.
- **Go adds an `error` return to every `async` effect**, whether or not the Gust
  declaration is a `Result`. The `(T, error)` idiom is how Go expresses "this can
  fail", and an `async` effect is assumed to be able to. Rust does not: an
  `async effect f() -> String` yields exactly a `String`.

The generated call sites follow. Where Go's method returns an `error`, the
transition method checks it and returns early:

```go
w, err := effects.AsyncValue(ctx, v)
if err != nil {
    return err
}
```

Rust has no such insertion. A `Result`-returning effect hands you the `Result`
and it is yours to match.

### `Result` in the Go target {#result-in-the-go-target}

Go has no `Result`. An effect declared `-> Result<T, E>` becomes a method
returning `(T, error)`, whether or not the effect is `async`:

```go
type OrderChecksEffects interface {
    // gust:effect -- replay-safe / idempotent
    ValidateOrder(order_id string) (string, error)
}
```

An `Ok`/`Err` match on the result lowers to a nil check on that `error`. Go's
idiom carries a single `error`, so `E` itself is erased: when `E` is `String` the
`Err` binding receives the error message, which is lossless. For any other `E`
the binding is a Go `error` and will not typecheck where `E` is expected — the
validator warns about that case so it surfaces against the `.gu` source rather
than as a Go compile error in generated output. Prefer `Result<T, String>` in
machines that target Go.

```
warning: Go cannot represent the error type of effect 'validate_order'
   = note: Go signals failure with a single `error`, so `Result<_, Failure>`
           lowers to `(_, error)` and the `Err` payload type is lost; the Rust
           backend is unaffected
   = help: declare the effect as `Result<_, String>` if this machine must also
           target Go
```

The warning fires only when an `Err` arm actually binds a name its own body
reads. A `Result` binding with no `Ok`/`Err` match keeps the plain early-return
lowering and loses nothing.

## Choosing `effect` or `action`

Use `effect` when repeating the operation is safe for the runtime's semantics.
Use `action` when repeating the operation could double-charge, duplicate a
notification, publish a second event, or otherwise change the outside world.

## Next

- [States and Transitions](states_transitions.md) — the `goto` that ends every
  handler path
- [Lifecycle](lifecycle.md) — how the effects parameter reaches the transition
  method
- [Errors](errors.md) — every diagnostic these rules can produce
