# Changelog

All notable changes to the Gust project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
