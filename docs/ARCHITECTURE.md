# Gust Compiler Architecture

## Overview

Gust is a source generator for type-safe state machines. It parses `.gu` files
into a typed AST, validates machine semantics, and emits host-language code for
multiple targets.

```text
source.gu
  -> pest parser
  -> AST with source spans
  -> validator and formatter
  -> Rust / Go / WASM / no_std / C FFI / JSON Schema codegen
```

The generated code is intentionally plain. Host applications own effect and
action implementations, while Gust owns the machine state, transition surface,
serialization shape, and runtime contracts.

## Workspace Structure

```text
gust/
  gust-lang/       core parser, AST, validator, formatter, and codegen
  gust-runtime/    runtime traits, envelopes, supervisors, and prelude
  gust-cli/        `gust` command-line tool
  gust-build/      Cargo build-script integration
  gust-lsp/        Language Server Protocol implementation
  gust-mcp/        MCP server exposing compiler operations
  gust-stdlib/     reusable .gu machines and EngineFailure
  editors/vscode/  VS Code extension assets
  docs/            mdBook source and architecture/spec documents
  examples/        runnable reference projects
```

## Pipeline Stages

### 1. Grammar

`gust-lang/src/grammar.pest` defines the language syntax:

- top-level `use`, `type`, `enum`, `channel`, and `machine` declarations
- machine states, transitions, `effect`, `action`, handlers, supervision, and
  lifecycle settings
- handler statements such as `let`, `if`, `match`, `goto`, `perform`, `send`,
  and `spawn`
- expressions with normal precedence, field access, calls, literals, and
  `perform` as an expression

The important design choice is that `perform` is both a statement and an
expression. That lets handlers bind effect/action results directly:

```gust
let receipt = perform charge_card(order_id, amount);
```

### 2. AST

`gust-lang/src/ast.rs` defines the typed intermediate representation:

- `Program`, `UsePath`, `TypeDecl`, `MachineDecl`
- `StateDecl`, `TransitionDecl`, `EffectDecl`, `OnHandler`
- `EffectKind::{Effect, Action}` for replay-safe versus externally visible
  operations
- `Statement` and `Expr` nodes, many carrying source spans for diagnostics

Generated-code annotations are derived from `EffectKind`, so downstream tools can
identify:

```text
gust:effect -- replay-safe / idempotent
gust:action -- not replay-safe / externally visible
```

### 3. Parser

`gust-lang/src/parser.rs` converts pest pairs into AST nodes. Parser failures are
reported as `GustError` values with line/column information and, where possible,
actionable help text.

### 4. Validator

`gust-lang/src/validator.rs` checks semantic rules before codegen:

- duplicate declarations and unknown states, effects, channels, or machines
- unreachable states and missing handlers
- invalid transition targets and `goto` arity/type mismatches
- effect/action arity and return-type checks where type information is known
- match exhaustiveness over known enums
- branch termination consistency
- action-safety rules for replay-aware runtimes

The validator is conservative around unknown host types: it avoids false
positives when the `.gu` source references types that only the host language
knows how to resolve.

### 5. Codegen

`gust-lang` contains target-specific generators:

| Target | Module | Output |
|--------|--------|--------|
| Rust | `codegen.rs` | `.g.rs` state enum, machine struct, transition methods, effects trait |
| Go | `codegen_go.rs` | `.g.go` state constants, data structs, transition methods, effects interface |
| WASM | `codegen_wasm.rs` | wasm-bindgen-oriented Rust wrapper surface |
| no_std | `codegen_nostd.rs` | heapless/alloc-friendly Rust |
| C FFI | `codegen_ffi.rs` | Rust exports plus companion C header |
| JSON Schema | `codegen_schema.rs` | schemas for types and machine states |

Rust and Go effect interfaces annotate every generated method with the stable
`gust:effect` or `gust:action` comment. This is the bridge used by workflow
runtimes that need to checkpoint non-idempotent operations without re-parsing
the original `.gu` source.

## Runtime

`gust-runtime` is intentionally small. It provides:

- `Machine` for current-state access and JSON round trips
- `Supervisor` and `SupervisorRuntime` for structured concurrency
- `ChildHandle`, restart strategies, and child task joining
- `Envelope<T>` for message payloads and correlation IDs

Network transport is intentionally out of scope for the current runtime.
Inter-machine communication is local/in-process.

## Tooling

`gust-cli` exposes the daily workflow:

- `build`
- `check`
- `fmt`
- `diagram`
- `watch`
- `init`
- `parse`
- `doctor`
- `schema`

`gust-build` lets Cargo projects compile `.gu` files from `build.rs`.
`gust-lsp` powers editor diagnostics, formatting, hover, symbols, completion,
signature help, code actions, references, and current-file rename behavior.
`gust-mcp` exposes compiler tools to AI-assisted development environments.

## Output Convention

Generated files use target-specific `.g.*` names and should not be edited by
hand:

```text
order_processor.gu          source
order_processor.g.rs        generated Rust
order_processor.g.go        generated Go
order_processor.g.wasm.rs   generated WASM-target Rust
order_processor.g.nostd.rs  generated no_std Rust
order_processor.g.h         generated C header for FFI target
```
