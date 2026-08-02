---
title: "Lifecycle"
description: "The life of a generated machine instance: construction, state inspection, transition invocation, async and timeout wrapping, and serialization."
type: reference
---

# Lifecycle

This page describes the API a `.gu` file produces and the order in which a host
application uses it. For the compiler's own pipeline — parse, validate, generate
— see [Code Generation](../advanced/codegen.md).

The machine used throughout:

```gust
machine Checkout {
    state Cart(total: i64)
    state Paid(total: i64, receipt: String)
    state Rejected(reason: String)

    transition pay: Cart -> Paid | Rejected

    effect charge(total: i64) -> String

    on pay(ctx: PayCtx) {
        if ctx.total > 0 {
            let receipt = perform charge(ctx.total);
            goto Paid(ctx.total, receipt);
        } else {
            goto Rejected("empty cart");
        }
    }
}
```

## 1. Construction

The constructor is derived from the **first declared state**. Its parameters are
that state's fields, in declaration order.

```rust
let mut machine = Checkout::new(4200);
```

```go
machine := NewCheckout(4200)
```

When the initial state has no fields, Rust also emits a `Default` impl, so
`Checkout::default()` works. When it has fields, no `Default` is emitted — there
is no meaningful default for them.

There is no other way to construct a machine in a chosen state. To resume from
persisted data, deserialize; see [step 5](#persistence).

## 2. Inspection

| | Rust | Go |
|---|---|---|
| Field | `pub state: CheckoutState` | `State CheckoutState` |
| Accessor | `machine.state() -> &CheckoutState` | read `machine.State` |
| Name | `format!("{:?}", machine.state())` | `machine.State.String()` |

The two backends represent the state differently, and the difference matters when
you read data out of it.

**Rust** emits one enum with a struct variant per state, so the state and its
data are one value:

```rust
if let CheckoutState::Paid { total, receipt } = machine.state() {
    println!("{receipt} for {total}");
}
```

**Go** has no sum type. It emits an `int` state tag with one constant per state,
a `String()` method over the tags, and a **separate nullable data struct per
state** hanging off the machine:

```go
if machine.State == CheckoutStatePaid {
    fmt.Println(machine.PaidData.Receipt, machine.PaidData.Total)
}
```

Every transition calls `clearStateData()` before populating the target's data, so
exactly one data pointer is non-nil at a time. Reading the wrong one panics.
Check the tag first.

## 3. Transitions {#transitions}

Each declared transition becomes one method. Calling it from any state other than
the transition's source state returns an error and leaves the machine unchanged.

```rust
machine.pay(&effects)?;
```

```go
if err := machine.Pay(effects); err != nil { /* ... */ }
```

### Transition methods {#transition-methods}

The signature is assembled from what the handler actually does. Nothing is added
speculatively.

| Included when | Rust | Go |
|---|---|---|
| always | `&mut self` | pointer receiver `(m *Checkout)` |
| the handler is `async`, or the transition declares a `timeout` | the method is `async` | leading `ctx context.Context` |
| the handler declares parameters with known types | those parameters, in order | the same, in order |
| the handler body contains `perform` | `effects: &impl CheckoutEffects` | `effects CheckoutEffects` |
| the handler body contains `spawn` | `supervisor: &SupervisorRuntime` | `supervisor SupervisorRuntime` |
| the handler body contains `send` | one `<channel>_tx: &Sender<T>` per channel | one `<channel>Ch *<Channel>` per channel |
| always | returns `Result<(), CheckoutError>` | returns `error` |

The `ctx` parameter is never in the generated signature — it is a source-level
accessor for the from-state's fields, and it is removed. See
[the `ctx` parameter](effects_handlers.md#the-ctx-parameter).

A handler that performs no effects takes no effects argument. That is why a
machine's methods often have different arities from one another:

```rust
machine.receive(event)?;      // no perform in this handler
machine.validate(&effects)?;  // this one performs
```

### Async and timeouts

`async` propagates from effects outwards. An `async effect` must be performed
from an `async on` handler, which makes the generated method `async` in Rust and
gives it a `context.Context` in Go.

A `timeout` on the transition has the same effect on the signature even when
nothing in the handler is async, because the timeout itself needs a runtime. The
Rust method becomes `async` and wraps the body in `tokio::time::timeout`; the Go
method takes a `ctx` and derives a `context.WithTimeout` from it. See
[Timeouts](states_transitions.md#timeouts).

## 4. Effects

A machine with at least one `effect` or `action` generates a
`<Machine>Effects` trait (Rust) or interface (Go). Implement it once and pass the
implementation to every transition method that needs it.

```rust
struct ProductionEffects;

impl CheckoutEffects for ProductionEffects {
    fn charge(&self, total: i64) -> String { /* ... */ }
}
```

Method shapes, including how `async` and `Result` change them, are tabulated
under [Signature lowering](effects_handlers.md#signature-lowering).

## 5. Persistence {#persistence}

Both backends make the whole machine — state tag plus data — serializable to
JSON, with matching field names.

**Rust** derives `Serialize` and `Deserialize` on the machine struct, the state
enum, and every declared type. Use `serde_json` directly:

```rust
let json = serde_json::to_string(&machine)?;
let restored: Checkout = serde_json::from_str(&json)?;
```

**Go** emits the round-trip helpers for you:

```go
data, err := machine.ToJSON()
restored, err := CheckoutFromJSON(data)
```

::: callout info "The `Machine` trait is not implemented for you"
`gust-runtime` defines a `Machine` trait with `current_state`, `to_json`, and
`from_json`. Generated code does **not** implement it — it derives the serde
traits and stops there. If you want `machine.to_json()`, write the `impl Machine
for Checkout` yourself; the associated type is the generated state enum and
`current_state` returns `&self.state`.
:::

## Required dependencies

Generated Rust is not self-contained. It emits `use serde::{Serialize,
Deserialize};` and `use gust_runtime::prelude::*;`, and derives
`thiserror::Error` for the machine's error enum. All three must be present:

```toml
[dependencies]
gust-runtime = "0.3"
serde = { version = "1", features = ["derive"] }
thiserror = "2"
```

Keep the `gust-runtime` version in step with the compiler that generated the
file. Add `tokio` as well if any machine declares a channel, a timeout, or an async
handler — those forms reference `tokio::` paths directly.

Codegen writes the file and does not touch your module tree. Wiring it in is
yours to do, and `include!` is safer than `#[path] mod`: `cargo fmt` follows
`mod` declarations and will silently reformat a `.g.rs`, which then fails
`gust generate --check` in CI.

## Next

- [States and Transitions](states_transitions.md) — the state space this API
  reflects
- [Errors](errors.md) — the error type every transition method returns
- [Effects and Handlers](effects_handlers.md) — implementing the effects trait
