# Changelog

All notable changes to the Gust project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Expression-level source spans** (#55, closes #46) — `Statement::If` and
  `Expr::BinOp` now carry `Span` values; if/else branch-termination and
  binary-operand diagnostics now point at the exact source location instead
  of falling back to `line: 0, col: 0`.
- **`action` keyword** (#40) — non-idempotent / externally visible counterpart
  to `effect`. Grammar, AST (`EffectKind::{Effect, Action}`), parser,
  formatter, codegen (rustdoc + Go `//` markers), and MCP (`kind` field on
  effects) all preserve the distinction. Replay-aware workflow runtimes
  (Corsac) consume `kind` to drive retry and checkpoint semantics.
- **Handler-safety diagnostics for actions** (#40) — two new warnings:
  (1) at most one `action` per code path; (2) an `action` must be the last
  side-effectful step before a transition.
- **`EngineFailure` in `gust-stdlib`** (#40) — typed runtime failure enum
  (`UserError`, `SystemError`, `IntegrationError`, `Timeout`, `Cancelled`)
  for workflow contracts. Importable via `use std::EngineFailure;`.
- **Goto field type validation** (#30) — `goto` argument types are checked
  against target state field types with conservative inference (unknown
  types skip the check rather than emitting a false positive).
- **Effect return type checking** (#30) — `let x: T = perform e(...)` is
  rejected when `T` doesn't match `e`'s declared return type.
- **If/else branch termination consistency** (#30) — warns when one branch
  of an `if/else` terminates and the other falls through.
- **Binary operator operand compatibility** (#30) — warns when a `BinOp`'s
  two operands resolve to incompatible concrete types.
- **Match exhaustiveness diagnostics** (#43) — warns on non-exhaustive
  `match` over known enums; exhaustive matches count as termination for
  handler fall-through analysis.
- **Effect argument arity validation** (#42) — `perform` invocations are
  checked against the effect's declared parameter count.
- **JSON Schema codegen** (#35) — `--target schema` / `gust schema` emits
  JSON Schema from types and machine states.
- **Optional tracing instrumentation** (#32) — `RustCodegen::with_tracing(true)`
  emits `tracing::info!` events guarded by a `tracing` feature flag.
- **`gust doctor`** (#27) — environment diagnostics for rustc, cargo, Go
  toolchains, project layout, and `.gu` file freshness.
- **Test coverage expansion** — unit tests for `gust-runtime` (45 tests),
  stdlib machines (121 tests), MCP integration (51 tests), LSP integration
  (85 tests), CLI integration (29 tests), build-script helper (31 tests),
  formatter roundtrip (12 tests), codegen edge cases (14 tests), and
  comprehensive diagnostics coverage (56 validator tests).
- **CLI integration coverage expanded** (#56) — CLI integration tests raised
  from 43% to 63% line coverage, covering edge cases in `build`, `check`,
  `fmt`, `diagram`, `parse`, and `doctor` subcommands.

### Changed

- **CI: coverage + audit jobs** (#52) — `cargo-llvm-cov` uploads to Codecov
  on every push; `cargo-audit` runs on every PR to catch known-vulnerable
  dependencies before merge.
- **Public API documented** (#57) — all public items in every crate carry
  rustdoc comments; `#![warn(missing_docs)]` is now enabled crate-wide so
  undocumented public items fail CI.
- **`.unwrap()` replaced with `.expect(GRAMMAR_INVARIANT)`** (#53) — parser
  infallible-path unwraps are replaced with structured `expect` messages;
  error render branches added to test coverage.
- **Source span tracking** (#13) — the validator now uses AST-carried
  `Span` values directly instead of the fragile `SourceLocator` string
  search. Replaces a known limitation called out in CLAUDE.md.
- **MCP `gust_parse` output** — effect entries now include a `kind` field
  (`"effect"` or `"action"`) alongside `name`, `params`, `return_type`,
  and `is_async`.
- **Stdlib `all_sources()`** now returns 7 entries (adds `engine_failure.gu`).
- **Clippy 1.95 compatibility** (#41) — collapsed nested match-arm guards
  to satisfy the stricter `collapsible_match` lint.

### Fixed

- **Broken intra-doc link** (#58) — resolved a broken `[foo]` rustdoc link
  and added `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links"` to CI so
  future breakage fails the build.
- **WASM and no_std codegen coverage** (#54) — raised to ~97% line coverage,
  catching several untested edge cases in target-specific codegen paths.
- **Build-script error handling** (#21) — replaced `unwrap()` with descriptive
  `expect()` messages throughout the test suite and expanded coverage to
  all five codegen targets.

### Known limitations (tracked as follow-ups)

- Gust enum variants support positional payloads only (e.g.
  `Variant(String, i64)`), not named fields. The `EngineFailure` type in
  the stdlib is documented with position meanings in its `.gu` source.

## [0.1.0] - 2025-06-15

Initial public release of Gust, a type-safe state machine language that compiles
`.gu` source files to idiomatic Rust and Go.

### Added

- **Core language**: PEG grammar (`grammar.pest`), parser, and strongly-typed AST
  supporting machines, states, transitions, handlers, effects, enums, guards, and
  `perform` expressions.
- **Rust codegen** (`--target rust`): generates idiomatic `.g.rs` files with
  `serde` serialization, effect traits, and `gust-runtime` integration.
- **Go codegen** (`--target go`): generates `.g.go` files with struct-based
  state machines and interface-based effects.
- **WASM codegen** (`--target wasm`): generates `wasm-bindgen`-annotated Rust
  for browser and edge deployments.
- **`no_std` codegen** (`--target nostd`): generates `no_std`-compatible Rust
  for embedded and resource-constrained targets.
- **C FFI codegen** (`--target ffi`): generates Rust code with `#[no_mangle]`
  C-ABI exports and a companion `.g.h` C header.
- **Validator** with rich diagnostics: undefined state/transition/effect errors
  with did-you-mean suggestions (powered by `strsim`), unreachable state
  detection, and match exhaustiveness checking for enums.
- **Source span tracking** on AST nodes for precise error locations.
- **Formatter** (`gust fmt`): comment-preserving, opinionated source formatter
  for `.gu` files.
- **CLI** (`gust-cli` crate) with subcommands:
  - `build` -- compile `.gu` files to the selected target.
  - `watch` -- file-watching mode with automatic recompilation.
  - `parse` -- dump the parsed AST for debugging.
  - `init` -- scaffold a new Gust project.
  - `fmt` -- format `.gu` source files.
  - `check` -- validate without generating code.
  - `diagram` -- generate Mermaid state diagrams from `.gu` files.
  - `doctor` -- environment diagnostics for toolchain verification.
- **Language Server** (`gust-lsp` crate) implementing LSP features:
  - Real-time diagnostics (parse errors and validation warnings).
  - Hover information with markdown-formatted type details.
  - Go-to-definition for states, transitions, and effects.
  - Document formatting via the built-in formatter.
  - Document symbols and workspace symbol search.
  - Signature help for transitions and effects.
  - Code actions (quick fixes from validator suggestions).
  - Inlay hints for state field types.
- **VS Code extension** (`gust-vscode`) with syntax highlighting, language
  server integration, and a custom `.gu` file icon.
- **MCP server** (`gust-mcp` crate) exposing five tools over JSON-RPC
  (stdin/stdout) for AI-assisted development: parse, validate, compile,
  format, and diagram generation.
- **Build-script helper** (`gust-build` crate) for compiling `.gu` files
  during `cargo build` with incremental compilation and `rerun-if-changed`
  tracking.
- **Runtime library** (`gust-runtime` crate) providing the `Machine` trait,
  `Supervisor`/`SupervisorRuntime` structured concurrency primitives,
  `Envelope` message type, and `RestartStrategy` (OneForOne, OneForAll,
  RestForOne).
- **Standard library** (`gust-stdlib` crate) with six reusable machines:
  CircuitBreaker, Retry, Saga, RateLimiter, HealthCheck, and
  RequestResponse.
- **Example projects**: `event_processor`, `microservice`, and
  `workflow_engine` demonstrating real-world usage.
- **Documentation book** (mdBook) with language reference and getting-started
  guides.
- **CI pipeline** (`ci.yml`): format check, clippy with `-D warnings`,
  workspace tests, example tests, Go codegen smoke test, PR auto-labeling,
  and size labels.
- **Security policy** (`SECURITY.md`) for vulnerability reporting.

### Changed

- Upgraded `thiserror` from 1.0 to 2.0.
- Upgraded `notify` to 8 and `notify-debouncer-mini` to 0.7.
- Upgraded `colored` from 2 to 3.
- Extracted shared AST helpers into `codegen_common` module.
- Redesigned `.gu` file icon as pixelated wind dots.

### Fixed

- Formatter now preserves handler bodies and uses composite keys so handler
  and transition comments do not collide.
- Codegen rewrites `ctx.field` to direct field access in both Rust and Go
  backends.
- Go async effect errors are now surfaced instead of being silently discarded.
- Unused state field bindings are prefixed with `_` to suppress warnings.
- Effect trait parameters use `&str` instead of `&String` for String types
  and idiomatic Rust types throughout.
- LSP rename and find-references disabled until symbol resolution is
  scope-aware (avoids incorrect results).
- CLI `init` hardened for use inside Cargo workspaces.
- Parser hardened with property tests and failure regression coverage.
- Build-script helper improved error handling and incremental rebuild logic.
- Security review findings addressed (two passes).

[0.1.0]: https://github.com/Dieshen/gust/releases/tag/v0.1.0
