# Codegen backends

Five backends plus a schema emitter. They are **not symmetric**, and the asymmetries are the main hazard when adding a language form — it is easy to update Rust, feel done, and leave four backends silently emitting nothing.

## Inventory

| Backend | File | Output | Entry point |
|---|---|---|---|
| Rust | `codegen.rs` | `.g.rs` | `RustCodegen::generate(mut self, program) -> String` |
| Go | `codegen_go.rs` | `.g.go` | `GoCodegen::generate(mut self, program, package_name) -> String` |
| WASM | `codegen_wasm.rs` | `.g.wasm.rs` | `WasmCodegen::generate(&self, program) -> String` |
| no_std | `codegen_nostd.rs` | `.g.nostd.rs` | `NoStdCodegen::generate(&self, program) -> String` |
| C FFI | `codegen_ffi.rs` | `.g.ffi.rs` + `.g.h` | `CffiCodegen::generate(&self, program) -> (String, String)` |
| JSON Schema | `codegen_schema.rs` | `.json` | `SchemaCodegen::generate(program) -> String` |

The signatures themselves are inconsistent — Rust and Go consume `self`, WASM/no_std/FFI borrow it, FFI returns a **tuple** of source and header, and `SchemaCodegen` is an associated function taking no receiver at all. Go needs a package name nothing else does. Expect to handle each separately rather than behind one trait.

Shared helpers live in `codegen_common.rs`: `collect_known_types`, `collect_referenced_idents`, `detect_ctx_param`, `escape_string_literal`, `handler_used_channels`, `handler_uses_perform`, `handler_uses_spawn`, `has_timeout_transition`, `to_snake_case`, `to_pascal_case`. Mermaid diagram generation also lives there. **Put new cross-backend logic here** rather than duplicating it five times — `detect_ctx_param` is the model, since ctx resolution must be identical across backends or the same `.gu` produces different APIs.

## Reach and support

- **`gust build` reaches all five** via `--target rust|go|wasm|nostd|ffi`.
- **`gust.toml` manifests support only `rust`, `go`, and `schema`.** Adding a target to the manifest means extending `ManifestTargets` in `gust-cli/src/main.rs`.
- **`gust-build`'s `Target` enum** covers `Rust`, `Go { package_name }`, `Wasm`, `NoStd`, `Cffi` — no schema.

Three surfaces, three different sets of supported targets. A new backend needs wiring in each place you intend it to be reachable from.

## What differs, concretely

**Generated Rust is not standalone.** It emits `use serde::{Serialize, Deserialize}` and `use gust_runtime::prelude::*`, derives `Serialize`/`Deserialize`, and derives `thiserror::Error` for the machine error enum. Consuming crates need `gust-runtime`, `serde` with `derive`, and `thiserror`. **Generated Go has no runtime dependency at all** — it's self-contained.

**Unused locals.** Go rejects an unused local outright (`declared and not used`); Rust only warns. The same `.gu` can therefore build as Rust and fail as Go. This is why the validator warns about unused `let` bindings against the `.gu` source — the author hears it once, at the source, instead of as a backend-specific surprise (issue #100).

**Async traits.** The Rust backend desugars `async fn` in the generated public effect trait to return-position `impl Future` with an explicit `Send` bound. A plain `async fn` in a public trait trips `async_fn_in_trait`, because the trait alone doesn't promise the future is `Send`, and callers holding the machine across an await need that. Implementors can still write `async fn`.

**Three live Rust/Go divergences** where the same validated `.gu` produces compiling Rust and broken Go. All pass `gust check`; all verified against the real Go toolchain:

- **Bare source-state field references.** A handler reading `index` rather than `ctx.index` emits Go with `undefined: index`. The Rust backend resolves the bare name against the from-state; Go does not. Not conditional on statement shape — a preceding `let` does not fix it (tested).
- **`Result`-returning effects matched with `Ok`/`Err`.** Go lowers such an effect into the `(value, err)` idiom with an automatic early return, so a following match emits `undefined: Ok` / `undefined: Err`.
- **Generic machines.** `machine Box<T>` emits `cannot use generic type BoxFullData[T any] without instantiation` — the emitted Go references the generic data struct without type arguments. A concrete version of the same machine compiles, isolating generics as the cause. **Underneath this sits a shared, backend-independent bug**: `detect_ctx_param` treats a machine's own generic parameter as an unknown type and therefore as the ctx marker, so `on put(value: T)` loses its parameter **in Rust too** — verified on 0.3.0, `pub fn put(&mut self)` with no `value`. Fixing the Go symptom without the shared cause leaves generic machines broken on both backends.

Together these make **`gust-stdlib` Rust-only**: `saga.gu`, `retry.gu`, `circuit_breaker.gu`, and `rate_limiter.gu` each hit at least one, and `rate_limiter.gu` hits two at once.

These read as codegen bugs rather than intended asymmetries — the Rust backend accepts forms the Go backend silently miscompiles, and nothing diagnoses it. Two complementary fixes: `codegen_backends.rs` fixtures for the bare-field form, a `Result` match, and a generic machine would pin the behavior whichever way it's resolved (a stdlib machine would make a good fixture, since those exercise all three); and a validator warning — "this construct does not lower to Go" — is the cheaper interim fix, matching the existing precedent of warning at the Gust level when backends disagree.

**no_std and WASM** are the most constrained and most often forgotten. They have dedicated coverage suites (`nostd_codegen_coverage.rs`, `wasm_codegen_coverage.rs`) because they were the backends that had drifted.

**C FFI** emits both a Rust source file and a C header. They must stay consistent — a form added to one and not the other produces a header that lies about the ABI.

## Checklist for a new language form

1. Implement in all five backends, or make the omission explicit with a comment explaining why the form doesn't apply.
2. If the form affects types or state shape, update `codegen_schema.rs` too.
3. Put shared derivation logic in `codegen_common.rs`.
4. Add a `codegen_backends.rs` fixture — the only test that proves the output compiles.
5. Check the FFI header, not just the FFI source.
6. If the form should be reachable from a manifest, extend `ManifestTargets`; from `build.rs`, extend `gust-build`'s `Target`.

## Known defects in emitted Rust

**`use tokio;` fails `clippy -D warnings`.** Any machine with a `channel` or a `timeout` puts `use tokio;` in the generated prelude (`codegen.rs:115`), which trips `clippy::single_component_path_imports`:

```
error: this import is redundant
 --> src/slow.g.rs:4:1
  = note: `-D clippy::single-component-path-imports` implied by `-D warnings`
```

Every `tokio` reference in the output is fully qualified, so the import is genuinely redundant and **deleting it is the correct fix**. The reason it has survived is that no test fixture had a channel or a timeout *and* ran clippy over the result — the gap the `codegen_backends.rs` table exists to close. Worth noting the blast radius: this breaks CI for any consumer running `clippy -D warnings`, on a file they are told never to edit.

An `#[allow]` in the prelude is the conservative alternative if deletion risks edition-2015 consumers, but verify that concern before choosing it over the simpler fix.

## Naming and hygiene

Generated files use `.g.rs` / `.g.go`, inspired by C# source generators, and carry a header comment (`Generated by Gust compiler` for Rust, `Code generated by Gust compiler` for Go — `docs_snippets.rs` asserts on these strings, so don't reword them casually).

Indentation in emitted code is deliberately not asserted on anywhere, and no consumer toolchain cares — `rustc`, `clippy`, and `go vet` all accept whatever is emitted. This is why indent bookkeeping is excluded from mutation testing. Don't add tests that pin generated whitespace; they're pure maintenance cost.
