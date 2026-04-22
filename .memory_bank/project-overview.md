# Gust Project Overview

## What is Gust?
Type-safe state machine language that compiles `.gu` files to idiomatic Rust and Go.
Compiler written in Rust as a Cargo workspace.

## Pipeline
`source.gu -> Parser (pest PEG) -> AST -> Validator -> Codegen -> .g.rs / .g.go`

## Workspace Crates
| Crate | Purpose |
|-------|---------|
| gust-lang | Core: grammar, parser, AST, validator, all codegens (Rust, Go, WASM, no_std, FFI) |
| gust-runtime | Runtime traits (Machine, Supervisor, Envelope) for generated Rust code |
| gust-cli | `gust` binary: build, watch, parse, init, fmt, check, diagram |
| gust-lsp | LSP server (tower-lsp): diagnostics, hover, go-to-def, formatting, symbols, etc. |
| gust-mcp | MCP server (JSON-RPC): 5 tools for AI-assisted development |
| gust-build | Cargo build.rs integration for compiling .gu files during cargo build |
| gust-stdlib | 6 reusable .gu machines: circuit breaker, retry, saga, rate limiter, health check, request-response |

## Key Design Decisions
- `perform` is an expression (allows `let x = perform effect(args)`)
- Generated files use `.g.rs` / `.g.go` extension
- Effects generate `{Machine}Effects` trait
- Goto args positionally zipped with target state fields
- Validator uses string search for spans (known limitation)
- LSP rename/find-references disabled in v0.1.0
- Examples excluded from workspace (`exclude = ["examples/*"]`)

## Release Status: v0.1.0
- Grammar, parser, AST complete
- Rust and Go codegen complete
- Additional targets: WASM, no_std, FFI
- CLI with all subcommands
- LSP with most features
- VS Code extension with syntax highlighting, snippets, commands
- MCP server with 5 tools
- Standard library with 6 machines
- Documentation book (mdBook)

## Repository
- GitHub: Dieshen/gust
- License: MIT
- CI: fmt, clippy -D warnings, workspace tests, example tests, Go smoke test
- Branch: master (default)
