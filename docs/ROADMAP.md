# Gust Roadmap

The canonical roadmap lives at [`../ROADMAP.md`](../ROADMAP.md). This file is a
docs-local summary for readers browsing the `docs/` tree.

## Current State

**Current release:** `v0.2.1`

Gust is a type-safe state machine DSL that compiles `.gu` sources to Rust, Go,
WASM-oriented Rust, `no_std` Rust, and C FFI-oriented Rust. The current release
includes the compiler, CLI, Cargo build integration, runtime support, LSP,
VS Code extension assets, MCP server, standard-library machines, mdBook docs,
and release-ready crate metadata.

The `v0.2.x` line is focused on production hardening for Gust as a state-machine
language:

- stronger validator diagnostics and source locations
- workflow-runtime semantics with `effect`, `action`, handler-safety warnings,
  and `EngineFailure`
- generated `gust:effect` / `gust:action` annotations for runtimes such as
  Corsac
- JSON Schema generation and `gust doctor`
- broader tests across CLI, parser, validator, codegen targets, LSP, MCP,
  runtime, stdlib, examples, and docs snippets

## Near-Term Remaining Work

These items are still the credible short path before larger language expansion:

- finish the remaining fine-grained source-span coverage for statement and
  expression nodes that still fall back to coarse locations
- add `gust test` for machine-level tests with mock effects
- add multi-file type resolution for project-local `use` declarations
- add cross-file LSP go-to-definition through imported `.gu` files
- define the VS Code extension bundling/release story for `gust-lsp`
- decide whether `gust_new` belongs inside `gust-mcp` or should remain in the
  external plugin/agent workflow

## Longer Horizon

The long-term roadmap explores Gust growing from a state-machine DSL into a
broader language while keeping machines first-class. The next major areas are:

- top-level functions and standalone modules
- expression-oriented control flow
- explicit mutation semantics
- contracts and state/version compatibility
- a first-class type and effect system beyond host-language errors
- research-track formal verification and incremental compilation

See [`../ROADMAP.md`](../ROADMAP.md) for the detailed phase list.
