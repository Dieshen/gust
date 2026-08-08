---
title: "Types"
description: "Declaring structs and enums in Gust, the type expressions the grammar accepts, and what each one becomes in Rust and Go."
type: reference
---

# Types

Gust has no type system of its own to speak of. It has a small vocabulary of
type *expressions*, a rule for declaring named types, and a deliberately
conservative checker. Everything else is delegated to the host language.

## Declaring types

### Structs

Structs use `type`, not `struct`:

```gust
type Order {
    id: String,
    total: i64,
    tags: Vec<String>,
}

machine Checkout {
    state Cart(order: Order)
    state Paid(order: Order)

    transition pay: Cart -> Paid

    on pay(ctx) {
        goto Paid(ctx.order);
    }
}
```

A trailing comma after the last field is allowed. Fields are ordered, and that
order is preserved in both backends.

### Enums

Enum payloads are **positional only**. Named payload fields do not parse.

```gust
enum Tier { Fast, Slow }

enum Failure { Timeout(i64), Rejected(String) }

machine Router {
    state Ready(tier: Tier)
    state Routed(tier: Tier)

    transition route: Ready -> Routed

    on route(ctx) {
        goto Routed(ctx.tier);
    }
}
```

This restriction is why the standard library's `EngineFailure` type is shaped
the way it is. If you need named payload data, declare a `type` and carry it as
a single positional payload.

A fieldless enum — one where every variant has an empty payload — is treated
specially by the Rust backend: it derives `Copy` as well as `Clone`, so reading
one out of a struct field is not a partial move.

## Type expressions {#type-expressions}

| Form | Example |
|---|---|
| Simple | `String`, `i64`, `Order`, `T` |
| Generic | `Vec<String>`, `Result<T, E>`, `Option<i64>` |
| Tuple | `(String, i64)` |
| Unit | `()` |

Nesting is unrestricted: `Vec<Result<Order, String>>` parses.

## What each type becomes

| Gust | Rust | Go |
|---|---|---|
| `String` | `String` | `string` |
| `i64` / `i32` | `i64` / `i32` | `int64` / `int32` |
| `u64` / `u32` | `u64` / `u32` | `uint64` / `uint32` |
| `f64` / `f32` | `f64` / `f32` | `float64` / `float32` |
| `bool` | `bool` | `bool` |
| `()` | `()` | `struct{}` |
| `Vec<T>` | `Vec<T>` | `[]T` |
| `Option<T>` | `Option<T>` | `*T` |
| `Result<T, E>` | `Result<T, E>` | `T`, plus a trailing `error` return |
| `(A, B)` | `(A, B)` | `struct { F0 A; F1 B }` |
| A declared `type` | a `struct` with `pub` fields | a `struct` with exported fields and `json` tags |
| A declared `enum` | a Rust `enum` | a `string` type with one constant per variant |

Struct fields keep their `.gu` names in Rust. In Go they are converted to
PascalCase and given a `json:"original_name"` tag, so the JSON representation
matches across both backends.

### `Result` is not symmetric

Go has no `Result`. An effect declared `-> Result<T, E>` becomes a method
returning `(T, error)`, and `E` is erased — Go's idiom carries exactly one error
type. When `E` is `String` the erasure round-trips through `err.Error()` and is
lossless. For any other `E` the validator warns, because the binding an `Err`
arm receives is a Go `error` and will not typecheck where `E` is expected.

Prefer `Result<T, String>` in any machine that must also target Go. The full
lowering, including how an `Ok`/`Err` match is translated, is on
[Effects and Handlers](effects_handlers.md#result-in-the-go-target).

### Names that are not builtins pass straight through

Any identifier is a valid type name. Undeclared names are emitted verbatim into
the target language, which is a feature — it lets a machine refer to a host type
that Gust knows nothing about — and a trap.

`HashMap<K, V>` is the clearest example. It is not a Gust builtin, so it is
passed through:

```
type Bag { items: HashMap<String, i64> }
```

- **Rust** emits `HashMap<String, i64>`. Generated files import only `serde` and
  `gust_runtime::prelude::*`, so this compiles only if the module that brings the
  file in has `HashMap` in scope. With `include!`, adding
  `use std::collections::HashMap;` beside it is enough.
- **Go** emits `HashMap[string, int64]`, which does not exist. There is no import
  that fixes it. Use a declared `type` or a `Vec` of pairs instead.

The same pass-through rule is what makes the [`ctx`
parameter](effects_handlers.md#the-ctx-parameter) work: `ctx: PayCtx` names a
type that is deliberately never declared. It also means a **typo in a type name
is not a compile error at the Gust level** — it is silently a new opaque type.

## Generics {#generics}

A machine may declare type parameters. Bounds are joined with `+` and are passed
through to Rust unchanged.

```gust
machine Cache<T: Clone> {
    state Empty
    state Full(value: T)

    transition put: Empty -> Full
    transition clear: Full -> Empty

    on put(value: T) {
        goto Full(value);
    }

    on clear() {
        goto Empty;
    }
}
```

- Effects cannot declare their own type parameters, but they may use the
  machine's: `effect get_step(steps: Vec<S>, index: i64) -> S`.
- A machine's own type parameters count as *known types*, so a handler parameter
  typed `T` is a real argument and not mistaken for the `ctx` accessor. This was
  not true in 0.3.0, where `on put(value: T)` silently generated `put(&mut self)`
  with the argument dropped.
- **Rust** puts the declared bounds on the type and adds `core::fmt::Debug` to
  the *impl* block, because the invalid-transition arm formats the state with
  `{:?}`. Non-generic machines are unaffected.
- **Go** lowers every parameter to `[T any]` — Gust bounds have no Go equivalent.
- The **ffi** backend does not lower handler bodies, so a generic machine's
  behaviour does not survive it.

## What the validator checks {#what-the-validator-checks}

Type inference is deliberately conservative. When the checker cannot determine a
type it skips the check rather than reporting a false positive.

Checked:

- `goto` argument types against the target state's declared field types.
- A `let` with both an explicit annotation and a `perform` right-hand side,
  against the effect's declared return type.
- Both operands of a binary operator, when both types are known.

Not checked:

- The return type of a plain function call — inference returns "unknown" and every
  dependent check is skipped.
- Anything involving a generic parameter. `is_generic_param` short-circuits
  compatibility to `true`, so `T` is treated as compatible with everything.
- Whether an undeclared type name exists in the host language.

## Next

- [Syntax](syntax.md) — where type expressions may appear
- [Effects and Handlers](effects_handlers.md) — how parameter types are borrowed
  in the generated effects trait
- [Errors](errors.md) — the exact diagnostics the type checks emit
