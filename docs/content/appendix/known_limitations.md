---
title: "Known Limitations"
description: "Where Gust and its backends currently stop, and what to do about each one."
type: reference
---

# Known Limitations

This page is deliberately blunt. Everything below is current behaviour, verified against the compiler, and grouped by whether it bites at parse time, at validation time, or only once the generated code reaches a real toolchain.

The last group is the dangerous one: `gust check` reports success and the failure surfaces later, in a file you were told never to edit.

## Language

### Enum variant payloads are positional only

The grammar's `variant` production takes a list of *type expressions*, with no name slot:

```text
variant = { ident ~ ("(" ~ type_expr ~ ("," ~ type_expr)* ~ ","? ~ ")")? }
```

So `Bar(String, i64)` parses and `Bar(name: String)` does not. The meaning of each position has to live in a comment. This is what shaped `EngineFailure` in the standard library, whose source documents every slot by hand.

### Enum variants with payloads cannot be constructed in an expression

`qualified_path` is exactly two identifiers with no argument list, and it is tried before `fn_call` in the ordered choice, so `EngineFailure::Timeout(500)` fails to parse on the `(`. Fieldless variants (`Status::Active`) are fine.

Build payload-carrying values in an effect and return them.

### No loops, methods, struct literals, or tuple values

None of these exist as productions. Iteration is modelled as a self-transition carrying an index, or pushed into an effect; computation the small expression language cannot express is declared as an effect and implemented in the host. The [Grammar](grammar.md) page lists the complete set of absences.

This is a design decision rather than a backlog item — it is what keeps a machine analysable and lets one source lower to both Rust and Go.

### String literals have no escape sequences

A string is "any run of characters that is not a quote", so the first `"` ends it. There is no `\"`, and `\n` is a backslash followed by `n`. If you need a quote or a newline inside a string, produce it in an effect.

### Handler return types parse but are rejected

`on go() -> i64 { ... }` is admitted by the grammar and then refused by the validator with *"handler return types are not yet supported"*. A handler communicates by transitioning, not by returning.

## Diagnostics

### Expression nodes do not all carry precise spans

Spans are attached to declarations, `goto`, `perform`, `send`, `spawn`, `let`, `if`, and binary operations. Other expression nodes fall back to a default span, so a diagnostic about one of them points at the enclosing statement rather than the exact sub-expression. Tracked as issue #46; coverage has been extended twice and is still incomplete.

### A misspelled parameter type silently changes a handler's signature

The `ctx` parameter is identified as the first handler parameter whose type is **not** a declared type, and it is then removed from the generated method. Undeclared type names are legal by design — that is how the idiom works — so a typo in a parameter's type makes that parameter the from-state accessor and drops it from the generated code. `gust check` reports success, because there is nothing here it can distinguish from the intended idiom.

If a handler argument mysteriously vanishes from generated code, suspect a misspelled type on the first parameter.

## Backends {#backends}

`gust check` validates the source. It does not promise that any particular backend's output compiles, and the backends are not equivalent.

### WASM cannot express generics at all

`#[wasm_bindgen]` rejects type parameters outright — *"structs with #[wasm_bindgen] cannot have lifetime or type parameters currently"*. A generic machine therefore has no representation on this backend, so every generic machine in `gust-stdlib` is out of reach from WASM.

This is a limit of `wasm-bindgen`, not an emitter bug, and there is no fix on the Gust side. The backend test matrix lists the generic fixtures as unsupported explicitly rather than skipping them, so the gap stays visible.

### Go erases a non-`String` error type in `Result<T, E>`

Go signals failure with a single `error` value, so an effect declared `-> Result<T, E>` lowers to Go's `(T, error)` idiom and `E` is lost. When `E` is `String` it round-trips: the `Err` binding receives `err.Error()`. Any other `E` leaves the binding holding a Go `error`, which will not typecheck where `E` was expected.

The validator warns rather than errors, because the same source is valid Rust. The warning only fires when an `Err` arm actually binds a name the handler reads — an ignored payload costs nothing.

If a machine must target Go, keep `Result` error types as `String`.

### `no_std` is a skeleton

The `no_std` backend emits the state enum, the user type declarations, and transition methods that construct the target state. It emits **no handler bodies and no effects trait**. Transition methods take the target state's fields as parameters, because there is no handler to compute them and no effect to fetch them.

Treat it as a typed state container for embedded targets, not as a port of the full runtime.

### A `channel` trips `clippy::new_without_default` on Rust

The generated channel struct has a `new()` with no accompanying `Default` impl, which is a hard error for a consumer building with `-D warnings`. Tracked as issue #110. The backend test matrix currently lists the `channel` fixture as unsupported for the `-D warnings` Rust configuration.

### C FFI generates a header that CI does not compile

The `ffi` backend emits Rust with `#[no_mangle]` C-ABI exports plus a companion `.g.h` header. Only the Rust half is compiled in CI — verifying the header would need a C toolchain in the pipeline. The header is generated from the same AST, but it is not machine-checked.

## Tooling

### Nested Cargo workspaces and `gust init` {#gust-init-workspaces}

`gust init <name>` detects a parent Cargo workspace and adds an empty `[workspace]` table to the generated `Cargo.toml` so the nested project builds standalone. Project names must be Cargo-compatible: `[A-Za-z0-9_-]+`.

If you scaffolded a project before that behaviour existed and it fails on workspace nesting, either add the empty `[workspace]` table yourself or move the project outside the parent workspace.

`gust init` also scaffolds path dependencies that only resolve inside the Gust repository. Replace them with registry versions for a standalone project.

### LSP rename and find-references are disabled

Neither is advertised by the language server. The symbol model is not scope-aware enough to guarantee a safe edit across identifiers, comments, and string literals, and a rename that silently corrupts a comment is worse than no rename. Both return once symbols resolve structurally rather than textually.

### `gust-build` is mtime-gated

The build-script helper rebuilds a `.gu` only when its timestamp is newer than the generated output. After a fresh clone every file shares one checkout timestamp, so a stale committed `.g.rs` is never rewritten — the drift only surfaces when someone happens to touch the `.gu`.

If you commit generated output, verify it in CI. The repository does this with `scripts/regen-generated.sh`, which defines how each committed file is produced and fails the build on any diff.

### Inter-machine transport is in-process only

Channels and supervision are local. Cross-process and network transport are deliberately deferred rather than partially implemented.

## Next steps

- [Grammar](grammar.md) — the complete set of forms, and what is absent from it
- [FAQ](faq.md) — shorter answers to the questions these limitations raise
- [Changelog](changelog.md) — when each of these last changed
