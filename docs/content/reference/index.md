---
title: "Reference"
description: "The complete surface of the Gust language: every form it accepts, every form it rejects, and what each one lowers to in Rust and Go."
type: reference
---

# Reference

This section describes the Gust language as the compiler implements it. It is
derived from `gust-lang/src/grammar.pest`, `validator.rs`, and the code
generators, and every claim on these pages was checked against a build of
`master`.

Two things make Gust unlike the language it resembles.

**Gust is much smaller than Rust.** There are no loops, no method calls, no
struct literals, no references, and no string escapes. Match arms take blocks
and carry no separating commas. Effect declarations must state a return type.
These are deliberate: the expression language is kept small so machines stay
analyzable and so the same source can lower to two very different host
languages. Writing `.gu` by extrapolating from Rust habits reliably produces
code that does not parse. Start at [Syntax](syntax.md#what-you-cannot-write).

**`gust check` is necessary, not sufficient.** It validates the `.gu` source.
It does not promise that the generated code compiles, and the backends are not
equivalent. The forms that pass validation and then break a target are listed
in [Errors](errors.md#what-gust-check-does-not-catch).

## Pages

| Page | Covers |
|---|---|
| [Syntax](syntax.md) | Every declaration, statement, and expression form; the forms Gust rejects |
| [Types](types.md) | `type`, `enum`, type expressions, generics, and how each type maps to Rust and Go |
| [States and Transitions](states_transitions.md) | `state`, `transition`, `goto`, timeouts, the initial state rule |
| [Effects and Handlers](effects_handlers.md) | `effect`, `action`, `perform`, `on` handlers, and the `ctx` parameter |
| [Channels](channels.md) | `channel`, `send`, the `sends` / `receives` annotations |
| [Supervision](supervision.md) | `supervises`, `spawn`, restart strategies |
| [Lifecycle](lifecycle.md) | Constructing, inspecting, driving, and persisting a generated machine |
| [Errors](errors.md) | Compile-time diagnostics, runtime error types, and the limits of validation |

## The three rules worth memorizing

1. **The `ctx` parameter is detected by type, not by name.** A handler parameter
   whose type is not a declared type becomes the from-state accessor and is
   removed from the generated method signature. A misspelled type name silently
   deletes a parameter. See
   [the `ctx` parameter](effects_handlers.md#the-ctx-parameter).
2. **`goto` arguments are positional.** They zip against the target state's
   declared fields in order, and only the count is checked before the types are.
   See [`goto`](states_transitions.md#goto).
3. **Anything the expression language cannot say becomes an effect.** There is no
   `.len()`, no indexing, and no struct construction. Declare an effect and
   implement it in the host language. See
   [the effect escape hatch](effects_handlers.md#the-effect-escape-hatch).

## A complete machine

Everything in this section is a detail of the following shape.

```gust
type Order { id: String, total: i64 }

machine Checkout {
    state Cart(order: Order)
    state Paid(order: Order, receipt: String)
    state Rejected(reason: String)

    transition pay: Cart -> Paid | Rejected

    effect charge(order: Order) -> String

    on pay(ctx) {
        if ctx.order.total > 0 {
            let receipt = perform charge(ctx.order);
            goto Paid(ctx.order, receipt);
        } else {
            goto Rejected("empty order");
        }
    }
}
```

User types use `type`, not `struct`. Each `state` optionally carries fields. A
`transition` names one source state and one or more `|`-separated targets. An
`effect` declares work the host application performs. An `on` handler runs when
the host calls the generated transition method, and every path through it should
end in `goto`.

::: callout info "Version"
This documents Gust 0.4.0 (unreleased). Several cross-backend gaps present in the
published 0.3.0 — bare source-state field references, `Result` matching, and
generic machines all breaking the Go target — are fixed on `master`. Verify
behavior against the compiler you are actually running, not against the last
release.
:::
