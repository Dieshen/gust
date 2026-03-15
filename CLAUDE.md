# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Gust?

Gust is a type-safe state machine language that compiles `.gu` source files to idiomatic Rust and Go. The compiler is written in Rust as a Cargo workspace.

## Build & Test Commands

```bash
# Build entire workspace
cargo build --workspace

# Run all workspace tests
cargo test --workspace --all-targets --all-features

# Run tests for a single crate
cargo test -p gust-lang
cargo test -p gust-runtime

# Run a specific test by name
cargo test -p gust-lang -- test_name

# Run a specific integration test file
cargo test -p gust-lang --test docs_snippets
cargo test -p gust-lang --test language_semantics

# Lint and format (CI enforces these)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Example project tests (excluded from workspace, run separately)
cargo test --manifest-path examples/event_processor/Cargo.toml
cargo test --manifest-path examples/microservice/Cargo.toml
cargo test --manifest-path examples/workflow_engine/Cargo.toml

# Go codegen smoke test
gust build examples/order_processor.gu --target go --output _tmp --package smoke
cd _tmp && go mod init smoke && go vet ./...
```

## Architecture

Pipeline: `source.gu → Parser (pest PEG) → AST → Validator → Codegen → .g.rs / .g.go`

### Workspace Crates

| Crate | Role |
|-------|------|
| `gust-lang` | Core compiler: PEG grammar (`grammar.pest`), parser, AST, validator, and all code generators (Rust, Go, WASM, no_std, C FFI) |
| `gust-runtime` | Thin runtime traits (`Machine`, `Supervisor`, `Envelope`) imported by generated Rust code |
| `gust-cli` | The `gust` binary — subcommands: `build`, `watch`, `parse`, `init`, `fmt`, `check`, `diagram` |
| `gust-lsp` | Language Server (tower-lsp) — diagnostics, hover, go-to-definition, formatting |
| `gust-mcp` | MCP server (JSON-RPC over stdin/stdout) — exposes compiler tools for AI-assisted development |
| `gust-build` | Cargo build-script helper (`build.rs` integration) for compiling `.gu` files during `cargo build` |
| `gust-stdlib` | Reusable `.gu` machines (circuit breaker, retry, saga, rate limiter, etc.) |

### Key Files in gust-lang

- `grammar.pest` — PEG grammar defining Gust syntax
- `ast.rs` — Strongly-typed AST node definitions
- `parser.rs` — Pest pairs → AST conversion; each grammar rule has a `parse_*` function
- `validator.rs` — Semantic validation with diagnostics and suggestions (uses `strsim` for did-you-mean)
- `codegen.rs` — Rust code generator (`RustCodegen`)
- `codegen_go.rs` — Go code generator (`GoCodegen`)
- `codegen_wasm.rs` / `codegen_nostd.rs` / `codegen_ffi.rs` — Additional target backends
- `codegen_common.rs` — Shared codegen utilities (Mermaid diagram generation lives here)
- `format.rs` — Gust source formatter
- `error.rs` — Error types

### Design Decisions

- **`perform` is an expression**, not just a statement — allows `let x = perform effect(args)`.
- **Generated file extension**: `.g.rs` / `.g.go` (inspired by C# source generators). These files should never be manually edited.
- **Effect traits**: Each machine with effects generates a `{Machine}Effects` trait. Transition methods take `effects: &impl {Machine}Effects`.
- **Goto field mapping**: Arguments to `goto` are positionally zipped with the target state's declared fields.
- **Validator uses string search** for source spans rather than parser spans — known limitation.
- **LSP rename/find-references disabled** in v0.1.0 until symbol resolution is scope-aware.
- **Examples are excluded** from the workspace (`exclude = ["examples/*"]` in root `Cargo.toml`) and must be tested via explicit `--manifest-path`.

## Codegen Targets

The `--target` flag selects the backend: `rust` (default), `go`, `wasm`, `nostd`, `ffi`. Go codegen requires `--package <name>`.

## Commit Convention

Conventional Commits with optional scope: `feat(parser):`, `fix(lsp):`, `docs:`, `ci:`, `test:`, `refactor:`, `build:`.

## CI

The `ci.yml` workflow runs fmt check, clippy with `-D warnings`, workspace tests, example tests, and a Go codegen smoke test. PRs also get auto-labeling (by crate) and size labels.
