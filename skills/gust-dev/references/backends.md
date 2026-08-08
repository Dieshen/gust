# Codegen backends

Two shipped backends plus a schema emitter, and an unstable C FFI. They are **not symmetric**, and the asymmetries are the main hazard when adding a language form — it is easy to update Rust, feel done, and leave another backend silently emitting nothing.

`codegen_wasm.rs` and `codegen_nostd.rs` were **deleted in 1.0**. Both emitted output that compiled without implementing the source machine — `wasm` dropped state payload fields, every handler body, and every effect — which is not something a stability promise can be extended over. To target WebAssembly, compile the **Rust** backend's output to `wasm32` and implement the generated effects trait in the host: strictly more capable, and it supports generic machines, which `#[wasm_bindgen]` never could.

## Inventory

| Backend | File | Output | Entry point |
|---|---|---|---|
| Rust | `codegen.rs` | `.g.rs` | `RustCodegen::generate(mut self, program) -> String` |
| Go | `codegen_go.rs` | `.g.go` | `GoCodegen::generate(mut self, program, package_name) -> String` |
| C FFI | `codegen_ffi.rs` | `.g.ffi.rs` + `.g.h` | `CffiCodegen::generate(&self, program) -> (String, String)` — requires `--unstable-ffi`, outside the 1.0 promise |
| JSON Schema | `codegen_schema.rs` | `.json` | `SchemaCodegen::generate(program) -> String` |

The signatures themselves are inconsistent — Rust and Go consume `self`, FFI borrows it and returns a **tuple** of source and header, and `SchemaCodegen` is an associated function taking no receiver at all. Go needs a package name nothing else does. Expect to handle each separately rather than behind one trait.

Shared helpers live in `codegen_common.rs`: `collect_referenced_idents`, `detect_ctx_param`, `escape_string_literal`, `handler_used_channels`, `handler_uses_perform`, `handler_uses_spawn`, `has_timeout_transition`, `to_snake_case`, `to_pascal_case`. Mermaid diagram generation also lives there. **Put new cross-backend logic here** rather than duplicating it per backend — `detect_ctx_param` is the model, since ctx resolution must be identical across backends or the same `.gu` produces different APIs. It is now a two-line check for a parameter with no type annotation; `collect_known_types` and `BUILTIN_TYPES` were deleted in 1.0 along with the rule that needed them.

## Reach and support

- **`gust build` reaches `rust`, `go`, and `ffi`** (the last behind `--unstable-ffi`). JSON Schema has its own subcommand, `gust schema`.
- **`gust.toml` manifests support only `rust`, `go`, and `schema`.** Adding a target to the manifest means extending `ManifestTargets` in `gust-cli/src/main.rs`.
- **`gust-build`'s `Target` enum** covers `Rust`, `Go { package_name }`, `Cffi` — no schema.

Three surfaces, three different sets of supported targets. A new backend needs wiring in each place you intend it to be reachable from.

## What differs, concretely

**Generated Rust is not standalone.** It emits `use serde::{Serialize, Deserialize}` and `use gust_runtime::prelude::*`, derives `Serialize`/`Deserialize`, and derives `thiserror::Error` for the machine error enum. Consuming crates need `gust-runtime`, `serde` with `derive`, and `thiserror`. **Generated Go has no runtime dependency at all** — it's self-contained.

**Unused locals.** Go rejects an unused local outright (`declared and not used`); Rust only warns. The same `.gu` can therefore build as Rust and fail as Go. This is why the validator warns about unused `let` bindings against the `.gu` source — the author hears it once, at the source, instead of as a backend-specific surprise (issue #100).

**Async traits.** The Rust backend desugars `async fn` in the generated public effect trait to return-position `impl Future` with an explicit `Send` bound. A plain `async fn` in a public trait trips `async_fn_in_trait`, because the trait alone doesn't promise the future is `Send`, and callers holding the machine across an await need that. Implementors can still write `async fn`.

**Three Rust/Go divergences, all closed in 1.0.** Each produced compiling Rust and broken Go from the same validated `.gu`, and each is now a fixture in `regressions.gu` whose generated Go is fed to `go vet`:

- **Bare source-state field references.** A handler reading `index` rather than `ctx.index` emitted Go with `undefined: index`; Rust resolved the bare name against the from-state and Go did not.
- **`Result`-returning effects matched with `Ok`/`Err`.** Go lowers such an effect to the `(value, err)` idiom with an automatic early return, so a following match emitted `undefined: Ok`.
- **Generic machines.** `machine Box<T>` emitted `cannot use generic type BoxFullData[T any] without instantiation`. Underneath sat a backend-independent bug: `detect_ctx_param` treated a machine's own generic parameter as an unknown type and therefore as the ctx marker, so `on put(value: T)` lost its parameter **in Rust too**. Removing that rule — the accessor is now the parameter with no annotation — fixed the shared cause rather than the Go symptom.

`gust-stdlib` was Rust-only as a consequence; `saga.gu`, `retry.gu`, `circuit_breaker.gu`, and `rate_limiter.gu` each hit at least one. All four now emit Go that vets clean.

The lesson is the durable part: these were **codegen bugs that nothing diagnosed**, because the tests asserted on emitted strings. `codegen_backends.rs` now compiles every fixture's output with its real toolchain, and `go_behaviour.rs` runs it — compiling proves output is well-formed, never that it implements the source machine.

**The one asymmetry that remains by design** is `Result<T, E>` where `E` is not `String`. Go has one `error` type, so `E` cannot survive the trip. `gust check` warns and passes, since the same source is valid Rust; every Go emission path refuses and writes nothing.

**C FFI** emits both a Rust source file and a C header. They must stay consistent — a form added to one and not the other produces a header that lies about the ABI.

## Checklist for a new language form

1. Implement in Rust and Go, or make the omission explicit with a comment explaining why the form doesn't apply. Decide about `ffi` too — it is unstable, not absent.
2. If the form affects types or state shape, update `codegen_schema.rs` too.
3. Put shared derivation logic in `codegen_common.rs`.
4. Add a `codegen_backends.rs` fixture — the only test that proves the output compiles.
5. Check the FFI header, not just the FFI source.
7. Add a `compat/` corpus entry and re-record goldens; a generated-output change needs a CHANGELOG entry, which CI enforces.
6. If the form should be reachable from a manifest, extend `ManifestTargets`; from `build.rs`, extend `gust-build`'s `Target`.

## Fixed: the redundant `use tokio;`

Any machine with a `channel` or a `timeout` used to put a bare `use tokio;` in the generated prelude, tripping `clippy::single_component_path_imports` and so breaking CI for any consumer running `clippy -D warnings` — on a file they are told never to edit. Every `tokio` reference in the output is fully qualified, so the import was genuinely redundant and deleting it was the correct fix.

It survived as long as it did because no fixture had a channel or a timeout *and* ran clippy over the result. That gap is what the `codegen_backends.rs` table closes: it compiles every fixture's output with the real toolchain, and the Rust row runs clippy with `-D warnings` because consumers do.

## Naming and hygiene

Generated files use `.g.rs` / `.g.go`, inspired by C# source generators, and carry a header comment (`Generated by Gust compiler` for Rust, `Code generated by Gust compiler` for Go — `docs_snippets.rs` asserts on these strings, so don't reword them casually).

Indentation in emitted code is deliberately not asserted on anywhere, and no consumer toolchain cares — `rustc`, `clippy`, and `go vet` all accept whatever is emitted. This is why indent bookkeeping is excluded from mutation testing. Don't add tests that pin generated whitespace; they're pure maintenance cost.
