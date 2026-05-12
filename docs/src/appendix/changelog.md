# Changelog

## v0.2.1

Patch release focused on workflow-runtime metadata and release hygiene.

### Highlights

- Generated Rust and Go effect interfaces now annotate every operation with
  stable `gust:effect` or `gust:action` comments
- Crates.io publish metadata is normalized across workspace crates
- `clap`, `assert_cmd`, and `tokio` maintenance updates are included
- CI coverage artifact upload now uses `actions/upload-artifact@v7`

## v0.2.0

Public release of Gust with workflow-runtime semantics, stronger
diagnostics, schema output, and broader test coverage.

### Highlights

- `action` keyword for non-idempotent, externally visible operations
- Handler-safety warnings for replay-aware runtimes
- `EngineFailure` in `gust-stdlib`
- Goto field type validation and effect return type checking
- Effect argument arity validation and match exhaustiveness diagnostics
- JSON Schema code generation via `--target schema` / `gust schema`
- `gust doctor` environment diagnostics
- Optional tracing instrumentation in Rust code generation

### Hardening Included in v0.2.0

- Public API rustdoc is enforced crate-wide
- Parser infallible paths use explicit `GRAMMAR_INVARIANT` expectations
- Expression-level source spans improve validator diagnostic locations
- CLI, LSP, MCP, runtime, stdlib, and codegen test coverage expanded
- Broken rustdoc intra-doc links fail CI

## v0.1.0

Initial public release of Gust with end-to-end language, tooling, and runtime support.

### Highlights

- Rust and Go code generation from shared `.gu` machine definitions
- CLI commands: `parse`, `build`, `watch`, `init`, `fmt`, `check`, `diagram`
- Validation and diagnostics improvements (state/effect/channel/machine checks)
- Async handlers/effects, enums, tuples, and `match` support
- Structured concurrency primitives (channels, supervision strategies, lifecycle timeouts)
- Additional targets: `wasm`, `nostd`, and `ffi`

### Hardening Included in v0.1.0

- Parser no longer panics on oversized numeric literals
- Channel config parsing correctly applies `capacity` and `mode`
- Runtime spawn/join race fixed in supervisor runtime
- String literal escaping hardened for generated Rust/Go output
- Regression coverage expanded with integration tests and parser property tests
- `gust init` now detects parent Cargo workspaces and auto-adds `[workspace]` for standalone nested project builds

### Known Limitations

- Projects scaffolded before workspace auto-detection may still require:
  - adding an empty `[workspace]` table to the generated `Cargo.toml`, or
  - moving the project outside the parent workspace
- Inter-machine communication is local in-process only (network transport deferred)
