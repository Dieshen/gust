---
title: "Errors"
description: "Every diagnostic gust check can emit, the runtime error types both backends generate, and the forms that pass validation and still break a target."
type: reference
---

# Errors

Gust reports failures at two moments: when the compiler reads your `.gu`, and
when the host application calls a transition method. This page covers both, and
then the gap between them — the forms that pass validation and still break a
backend.

## Compile-time diagnostics

`gust check <file>` parses and validates without generating anything. Run it
first.

### Anatomy

Diagnostics follow `rustc`'s shape: a severity and message, a file/line/column
pointer, two lines of source context with a caret, and optional `note` and `help`
lines.

```
error: undefined state 'Runnin' in transition target
  --> order.gu:9:5
   |
 8 |
 9 |     transition start: Idle -> Runnin
   |     ^
10 |
   |
   = note: declared states: Idle, Running, Done
   = help: did you mean 'Running'?
```

Name-resolution failures carry a did-you-mean suggestion when a declared name is
within a Levenshtein distance of 2. Set `NO_COLOR` to strip the ANSI colors.

Spans are precise for top-level nodes — declarations, `goto`, `perform`, `send`,
`spawn`. Expression-level nodes fall back to the enclosing handler's position, so
some diagnostics point at the `on` line rather than the offending expression.

### Errors

An error blocks code generation. `gust check` exits `1`.

| Message | Cause |
|---|---|
| `duplicate state name 'X'` | Two `state` declarations share a name |
| `duplicate transition name 't'` | Two `transition` declarations share a name |
| `undefined state 'X' in transition source` | The left-hand side of a transition is not a declared state |
| `undefined state 'X' in transition target` | A `->` target is not a declared state |
| `undeclared effect 'e'` | `perform e(...)` with no matching `effect` or `action` |
| `undeclared channel 'C'` | `send C(...)` naming a channel not declared at program scope |
| `undeclared channel 'C' in 'sends' annotation on machine 'M'` | A `sends` or `receives` annotation naming an undeclared channel |
| `undeclared machine 'M'` | `spawn M(...)` naming a machine not declared in the program |
| `goto 'X' expects N argument(s) but got M` | `goto` arity does not match the target's field count |
| `goto 'X' argument N has type A, but field 'f' expects B` | Inferred argument type conflicts with the field type |
| `goto target 'X' is not a declared target of transition 't'` | The transition does not list that target |
| `effect 'e' expects N argument(s) but got M` | `perform` arity does not match the declaration |
| `let 'x' annotated as A, but effect 'e' returns B` | An annotated `let` conflicts with the effect's return type |
| `binary operator 'op' has incompatible operand types: A vs B` | Both operand types are known and differ |
| `field 'f' not available in state 'S'` | `ctx.f` names a field the source state does not have |
| `handler return types are not yet supported` | `on go() -> i64 { ... }` |
| `return statements are not supported in handlers; use goto to transition` | A `return` inside a handler body |

### Warnings

A warning does not block generation and does not change the exit code. There is
no `--deny-warnings`; treat them as errors yourself.

| Message | Cause |
|---|---|
| `unreachable state 'S'` | No transition targets that state (the initial state is exempt) |
| `transition 't' has no handler` | A declared transition with no `on` block |
| `handler 't' has code paths that don't end with a goto` | Some path can fall through |
| `handler 't' has inconsistent if/else: the then branch transitions but the else branch may fall through` | One branch terminates and the sibling does not |
| `non-exhaustive match on enum 'E': missing variant(s) ...` | An enum match with neither full coverage nor a `_` arm |
| `unused effect 'e'` | Declared but never performed |
| `unused binding 'x'` | A `let` the handler never reads |
| `handler parameter 'p' is shadowed by the from-state field of the same name` | The destructured field wins; the parameter is unreachable |
| `handler 't' performs N actions in a single sequence` | More than one `action` on one path |
| `handler 't' has side-effectful steps after an action` | An `action` is not the last externally visible step before the transition |
| `Go cannot represent the error type of effect 'e'` | `Result<_, E>` where `E` is not `String`, destructured with a used `Err` binding |

Two of these are worth extra attention.

**`unused binding`** looks cosmetic and is not. Rust merely warns, but Go rejects
an unused local outright, so the same `.gu` builds for one target and fails the
other. Reporting it against the source means you hear it once. Both backends now
lower an unread binding to a discard so the output still compiles, but the
statement form (`perform log(msg);`) says what you mean.

**`handler ... doesn't end with a goto`** has a known false positive. The
termination check treats a `match` as exhaustive only when it has a `_` arm or
covers every variant of a *declared* `enum`. `Result` is a builtin, not a
declared enum, so an `Ok`/`Err` match that covers both cases still warns.

## Runtime errors

Every transition method returns an error rather than panicking when it is called
from the wrong state.

### Rust

Each machine generates a `thiserror`-derived enum:

```rust
#[derive(Debug, Clone, thiserror::Error)]
pub enum CheckoutError {
    #[error("invalid transition '{transition}' from state '{from}'")]
    InvalidTransition { transition: String, from: String },
    #[error("transition failed: {reason}")]
    Failed { reason: String },
}
```

- `InvalidTransition` — the machine was not in the transition's source state.
  `from` is the `Debug` formatting of the current state.
- `Failed` — currently produced only by a
  [transition timeout](states_transitions.md#timeouts), carrying
  `transition 'name' timed out after ...`.

Transition methods return `Result<(), CheckoutError>`. The state is unchanged on
either error.

### Go

Each machine generates one error struct covering both cases:

```go
type CheckoutError struct {
    Transition string
    From       string
    Message    string
}
```

`Error()` renders `invalid transition '%s' from state '%s'` when `Message` is
empty and `transition '%s' failed: %s` when it is not. Transition methods return
`error`; the concrete type is always `*CheckoutError`.

### Effect failures

The two backends differ, and the difference is not cosmetic.

- **Rust** hands you whatever the effect returned. An effect declared
  `-> Result<T, E>` yields that `Result` and it is your handler's job to match it.
  Nothing propagates automatically.
- **Go** inserts `if err != nil { return err }` after any effect call that
  returns an `error` — that is, every `Result` effect and every `async` effect —
  unless a following `Ok`/`Err` match consumes the result. The transition method
  therefore returns the effect's error directly, not a `*CheckoutError`.

An `Ok`/`Err` match suppresses the early return and becomes a nil check instead,
so both branches run in Go exactly where they run in Rust.

## What `gust check` does not catch {#what-gust-check-does-not-catch}

`gust check` validates the `.gu`. It does not promise the generated code
compiles, and the backends are not equivalent. The table below is the current
state of `master`; each row was verified by feeding generated output to the real
toolchain.

| Construct | Rust | Go | wasm |
|---|---|---|---|
| Bare source-state field reference (`index`, not `ctx.index`) | compiles | compiles | compiles |
| `-> Result<T, E>` matched with `Ok`/`Err` | compiles | compiles; `E` erased to `error`, warns unless `E` is `String` | compiles |
| Generic machine (`machine Box<T>`) | compiles | compiles | **rejected** — `wasm_bindgen` does not support type parameters |
| Unused `let` bound from `perform` | compiles (lowered to `let _`) | compiles (lowered to `_ =`) | compiles |
| A `channel` declaration | compiles | compiles | compiles |
| A `sends` annotation on a machine | compiles — the helper is an inherent method | compiles | — |
| `HashMap<K, V>` in a type | resolves only if the including module imports it | **does not compile** — `HashMap[K, V]` does not exist | — |
| Early `goto` with no `else` | compiles — `goto` returns | compiles | — |

Most of these rows were failures in the published 0.3.0 and are fixed on
`master`: the Go breakages, the `channel` and `sends` rows, and the early-`goto`
row, which on 0.3.0 fell through and usually failed with `borrow of moved
value`. If you are running the released compiler, consult its own release notes
rather than this table.

::: callout tip "The rule this implies"
Build and compile the output for every backend you actually ship, and run
`clippy -D warnings` rather than plain `cargo check` — consumers do. Gust's own
test suite learned this the hard way: three backends were emitting output no
compiler had ever seen, and two of the three did not compile.
:::

## Next

- [Syntax](syntax.md#what-you-cannot-write) — the forms that fail earlier, in the
  parser
- [Lifecycle](lifecycle.md#transitions) — where the runtime error types surface
- [Debugging](../guides/debugging.md) — working through a failure end to end
