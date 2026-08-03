---
title: "Changelog"
description: "Release history for Gust, mirroring the repository's CHANGELOG.md."
type: reference
---

# Changelog

Mirrors the repository's `CHANGELOG.md`, which follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Gust adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

::: changelog

== 0.4.0 — 2026-08-03

Published to crates.io across all seven crates. The theme of this cycle is that generated code is now compiled by a real toolchain on every backend — which is how most of the fixes below were found.

### Breaking

- **Go: a synchronous effect returning `Result<T, E>` now returns `(T, error)`.** It previously returned bare `T` and discarded the failure, leaving nothing for an `Err` arm to test. This changes the generated interface for such effects; no `.gu` in the repository declared one.
- **`goto` now ends the handler.** It previously emitted a bare state assignment and execution continued, so an early `goto` inside an `if` fell through — the machine ended in whichever state the *last* assignment named, with no diagnostic anywhere. Handlers written to work around the old behaviour still work; handlers that relied on falling through do not.
- **A machine with a `supervises` clause now generates a supervision contract**, and a handler containing `spawn` takes an extra `children` argument. Call sites change.
- **A machine type parameter that nothing references is now a validator error.** `machine CircuitBreaker<T>` and `machine RateLimiter<K>` each declared one; the emitted state enum failed `E0392`. Both stdlib machines dropped theirs.
- **`build`, `watch`, and `generate` validate before emitting.** A `.gu` with a semantic error now fails instead of producing output. Sources that "built" before may now fail.

### Fixed

- **Every backend's output is now compiled by its real toolchain.** A table-driven harness over (fixture × backend) replaces the old per-backend string assertions. `wasm`, `no_std`, and `ffi` had never had their output fed to a compiler; two of the three did not compile. Combinations a backend genuinely cannot handle are listed explicitly with their tracking issue rather than silently skipped.
- **Generated Rust no longer emits a redundant `use tokio;`.** Any machine with a `channel` or a `timeout` transition emitted the import, which trips `clippy::single_component_path_imports` — a hard error for any consumer building with `-D warnings`, in a file they are told never to edit. Every emitted `tokio` reference was already fully qualified.
- **Generated Rust passes `clippy -D warnings`.** Output compiled but tripped `redundant_field_names`, `cmp_owned`, and `new_without_default`. The codegen compilation test now runs clippy rather than `cargo check`.
- **Go: source-state fields can be read by bare name.** The Rust backend gets these from destructuring the from-state in its match arm; Go has no such arm and emitted `undefined: tokens` for a handler reading `tokens` rather than `ctx.tokens`. Fields the handler reads are now lifted into locals at the top of the transition method — only read fields, because Go rejects an unused local.
- **Go: `Result`-returning effects can be matched with `Ok`/`Err`.** Go has no `Result`, so a `match` on the binding emitted a `switch` over `undefined: Ok`. An effect declared `-> Result<T, E>` now lowers to `(T, error)` whether or not it is `async`, and an `Ok`/`Err` match lowers to a nil check. `E` is erased to Go's `error`; when `E` is `String` the `Err` binding receives `err.Error()`.
- **Go: `perform` of an `async` effect returning `()` binds one value.** It emitted `if _, err := effects.Notify(ctx)` against an interface method returning only `error` — an assignment-count mismatch.
- **Generic machines compile on both backends.** Go referenced the generated state-data struct without type arguments; Rust's invalid-transition arm formats the state with `{:?}`, and a generic state enum's derived `Debug` only applies when its type parameters are `Debug` — the transition impl now carries that bound.
- **A machine's generic parameters are no longer mistaken for a ctx accessor.** ctx detection treats an unrecognised type name as the marker for the from-state accessor, so `on put(value: T)` on a `machine Box<T>` had its parameter dropped from the generated signature, leaving every reference to it undefined in Rust as well as Go.
- Together these make **`gust-stdlib` usable from Go**: all seven sources now generate Go that `go build` accepts, where previously every one of them failed.
- **Unused bindings no longer produce uncompilable output** (#100). A `let` the handler never reads was emitted as a real binding — rejected outright by Go, and a failure under Rust's `-D warnings`. Both backends now lower an unread binding to a discard (`_ = expr` / `let _ = …`) while an async Go perform keeps its `_, err :=` error propagation.
- **`no_std` can construct states that carry fields** (#103). The transition emitter wrote `self.state = State::Variant;` with no field construction, so any state with fields produced `E0533`. Transition methods now take the target state's fields as parameters.
- **`no_std` emits user type declarations.** State enums referenced `type`-declared structs and enums that were never emitted, so output failed with `cannot find type`.
- **`wasm` output compiles.** Structs were emitted with plain `#[wasm_bindgen]`, whose generated getters return `pub` fields by value and therefore require `Copy`, so any struct with a `String`, `Vec`, or nested field failed. Structs now use `#[wasm_bindgen(getter_with_clone)]`, and the C-like state enums derive `Clone, Copy`.
- **Committed generated output was stale, and now cannot drift.** Seven of the eleven committed `.g.rs` / `.g.go` files predated several codegen changes. Nothing caught it: `gust-build` is mtime-gated, so after a fresh clone — where every file shares one checkout timestamp — it never rewrites a stale output. All eleven are regenerated, `scripts/regen-generated.sh` defines how each is produced, and CI fails on any diff.

### Added

- **`sends` / `receives` annotations are checked against declared channels.** A machine header naming a channel that does not exist was silently accepted, even though `send` to the same undeclared channel has always been a hard error. The annotation is not decoration — both backends iterate `machine.sends` to emit the send helpers, and a typo made the helper vanish from the generated API with no diagnostic anywhere. Now an error, with a did-you-mean suggestion.
- **Unused-binding diagnostic.** A `let` the handler never reads now warns. Rust warns, but Go rejects an unused local outright, so the same `.gu` produced a Go package that would not build.
- **Shadowed-handler-parameter diagnostic.** A handler parameter sharing a name with a field of the transition's from-state now warns. Codegen destructures the from-state inside the transition method, so the parameter is shadowed and unreachable, leaving a dead argument on the generated method.
- **Validator warning when `Result`'s error type cannot survive Go codegen.** `String` round-trips through `error.Error()`; any other `E` leaves the `Err` binding holding a Go `error`, which will not typecheck where `E` is expected. A warning rather than an error, since the same source is valid Rust, and only when an `Err` arm actually binds a name the handler reads.
- **Backend fixtures for bare source-state field reads, an `Ok`/`Err` match, a generic machine, and `gust-stdlib/retry.gu`.** The retry fixture exercises all three at once, which is what had made the standard library Rust-only. `wasm` is listed as unsupported for the two generic fixtures: `#[wasm_bindgen]` rejects type parameters outright, so this is a backend limit rather than an emitter bug.

### Changed

- **Transitions no longer deep-copy the whole state.** Every transition method opened with `match self.state.clone()`, copying all fields including `Vec` and `String` payloads, and doing so *before* the from-state check — so a rejected transition paid the cost too. Transitions now match on `&self.state` and clone only the fields the handler references; `Copy` fields are dereferenced rather than cloned.
- **No underscore-prefix exemption for the unused-binding warning.** `_name` warns like any other unread binding. Gust never documented such a convention, bare `perform f();` has been valid since the first commit, and Go accepts only a bare `_` — never `_name`.
- **`Statement::Let` carries a source span**, so unused-binding diagnostics point at the `let` rather than at the enclosing handler. Continues the span coverage tracked in #46.
- **Validator traversal into nested blocks is tested.** `cargo-mutants` showed that deleting the `if` / `match` recursion arms from six validators failed no test. Behaviour was already correct; nothing asserted it. Mutation score on `validator.rs` went from 28/43 caught to 41/45.
- **Two examples dropped dead bindings.** `event_processor` and `workflow_engine` each bound an effect result they never read; now bare `perform` calls.

### Security

- **`gust generate` output paths are confined.** A manifest's `[targets.*]` `output` could resolve anywhere the invoking user could write, via `..` segments or an absolute path. Since cloning a repository and running `gust generate` inside it is ordinary — and the `gust.toml` arrives with the repository — outputs must now resolve beneath either the manifest directory or the current directory, checked after normalising `.` and `..`. `--allow-outside` lifts the restriction. Affects 0.3.0.

== 0.3.0 (2026-07-27)

Contract packages, plus two generated-code fixes found by dogfooding Gust into a downstream project.

### Breaking

- **Async effect implementations must now be `Send`** (#99). Generated effect traits previously declared `async fn`, which places no auto-trait bound on the returned future. They now declare `-> impl Future<Output = T> + Send`. An implementation whose future is not `Send` — one holding an `Rc`, a `RefCell` borrow, or a non-`Send` client across an `.await` — compiled against 0.2.x and will fail after regenerating.

  The bound is what makes a machine usable from a spawned task, which is the ordinary case; most implementations already satisfy it. If you genuinely need a non-`Send` implementation, keep it off the await path.

### Added

- **Contract packages and `gust generate`.** A directory of shared `.gu` sources plus a `gust.toml` manifest can emit several targets in one run, which is the intended shape when Rust and Go projects consume the same contracts. The manifest declares a `[source]` root with include/exclude globs and any of `[targets.rust]`, `[targets.go]`, and `[targets.schema]`. Paths resolve relative to the manifest file, not the shell's working directory.

### Fixed

- **Fieldless enums derive `Copy`** (#99). Reading a fieldless enum out of a struct field partially moved the struct, so any later use failed with `E0382`.
- **Async effects no longer trip `async_fn_in_trait`** (#99). Implementors can still write a plain `async fn`; no signature change is needed on upgrade.

### Changed

- **Generated Rust is compile-tested against a trait implementor.** The codegen compilation test was `#[ignore]`d and so had never run in CI, and it only ever compiled emitted code, never code implementing a generated effect trait. Both bugs above were invisible as a result.

== 0.2.1 (2026-05-12)

Patch release focused on workflow-runtime metadata and release hygiene.

### Added

- **Generated effect/action annotations** (#75). Rust and Go effect interfaces now mark every declared operation with a stable comment: `gust:effect -- replay-safe / idempotent` or `gust:action -- not replay-safe / externally visible`. Replay-aware runtimes can consume generated code without re-parsing `.gu` sources.

### Changed

- Crates.io release metadata normalised across workspace crates.
- `clap`, `assert_cmd`, and `tokio` maintenance updates.
- CI coverage artifact upload moved to `actions/upload-artifact@v7` (#70).

== 0.2.0 (2026-04-21)

Workflow-runtime semantics, stronger diagnostics, schema output, and much broader test coverage.

### Added

- **`action` keyword** (#40) — the non-idempotent, externally visible counterpart to `effect`. Grammar, AST, parser, formatter, codegen, and MCP all preserve the distinction; replay-aware runtimes consume `kind` to drive retry and checkpoint semantics.
- **Handler-safety diagnostics for actions** (#40): at most one `action` per code path, and an `action` must be the last side-effectful step before a transition.
- **`EngineFailure` in `gust-stdlib`** (#40) — typed runtime failure enum for workflow contracts, importable via `use std::EngineFailure;`.
- **Goto field type validation** (#30) — `goto` argument types checked against target state field types, with conservative inference that skips unknown types rather than reporting a false positive.
- **Effect return type checking** (#30) — `let x: T = perform e(...)` is rejected when `T` does not match the declared return type.
- **If/else branch termination consistency** (#30) and **binary operand compatibility** (#30) warnings.
- **Match exhaustiveness diagnostics** (#43) — an exhaustive match counts as termination for fall-through analysis.
- **Effect argument arity validation** (#42).
- **JSON Schema codegen** (#35) via the `gust schema` subcommand.
- **Optional tracing instrumentation** (#32) — `tracing::info!` events behind a `tracing` feature flag.
- **`gust doctor`** (#27) — environment diagnostics for rustc, cargo, Go toolchains, project layout, and `.gu` freshness.
- **Expression-level source spans** (#55, closes #46) — `Statement::If` and `Expr::BinOp` carry spans, so diagnostics point at the real location instead of `line: 0, col: 0`. `Expr::Perform` gained one in the follow-up (#63).
- Substantial test coverage expansion across runtime, stdlib, MCP, LSP, CLI, build-script, formatter, and codegen.

### Changed

- **Public API documented** (#57) — `#![warn(missing_docs)]` crate-wide, so an undocumented public item fails CI.
- **Source span tracking** (#13) — the validator uses AST-carried spans directly instead of a fragile string search.
- **MCP `gust_parse` output** — effect entries include a `kind` field (`"effect"` or `"action"`).
- **`all_sources()`** returns 7 entries, adding `engine_failure.gu`.
- CI gained coverage and `cargo-audit` jobs (#52).

### Known limitations recorded at the time

- Gust enum variants support positional payloads only. `EngineFailure` documents its position meanings in its `.gu` source.

== 0.1.0 (2025-06-15)

Initial public release: language, tooling, and runtime end to end.

### Added

- **Core language** — PEG grammar, parser, and strongly-typed AST covering machines, states, transitions, handlers, effects, enums, and `perform` expressions.
- **Five codegen targets** — `rust` (serde, effect traits, `gust-runtime` integration), `go` (struct-based machines, interface-based effects), `wasm` (`wasm-bindgen`), `nostd`, and `ffi` (C-ABI exports plus a `.g.h` header).
- **Validator** with did-you-mean suggestions (`strsim`), unreachable-state detection, and match exhaustiveness checking.
- **Formatter** (`gust fmt`) — comment-preserving and opinionated.
- **CLI** — `build`, `watch`, `parse`, `init`, `fmt`, `check`, `diagram`.
- **Language Server** — diagnostics, hover, go-to-definition, formatting, document and workspace symbols, signature help, code actions, and inlay hints.
- **VS Code extension** with syntax highlighting and a custom `.gu` file icon.
- **MCP server** exposing five tools over JSON-RPC for AI-assisted development.
- **Build-script helper** (`gust-build`) with incremental compilation and `rerun-if-changed` tracking.
- **Runtime library** (`gust-runtime`) — the `Machine` trait, `Supervisor` primitives, `Envelope`, and `RestartStrategy`.
- **Standard library** (`gust-stdlib`) with six reusable machines.
- **Example projects** — `event_processor`, `microservice`, `workflow_engine`.

### Fixed

- Formatter preserves handler bodies and uses composite keys so handler and transition comments do not collide.
- Codegen rewrites `ctx.field` to direct field access in both backends.
- Go async effect errors are surfaced instead of silently discarded.
- Effect trait parameters use `&str` rather than `&String`.
- LSP rename and find-references disabled until symbol resolution is scope-aware.
- Parser hardened against oversized numeric literals, with property tests and failure regression coverage.

:::

## See also

- [Known Limitations](known_limitations.md) — the current state of everything the entries above have not finished
- [`CHANGELOG.md`](https://github.com/Dieshen/gust/blob/master/CHANGELOG.md) — the authoritative file in the repository
