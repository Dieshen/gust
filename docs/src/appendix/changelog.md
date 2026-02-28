# Changelog

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
