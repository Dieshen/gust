# Gust Project State

Last updated: 2026-04-21

## Overview

Gust is a type-safe state machine language that compiles `.gu` source files to idiomatic Rust and Go. Cargo workspace with 7 crates. **v0.2-tag-ready** — `CHANGELOG.md` `[Unreleased]` section holds the 0.2 body; rolling to `[0.2.0] - 2026-04-21` is a one-line edit when the user decides to cut.

## Workspace coverage

**85.95% lines, 79.12% functions** (up from 82.29% / 75.49% at start of the v0.2 pass).

## Crate Health

| Crate | Tests | Notes |
|-------|-------|-------|
| gust-lang | 300+ across 18 test files | validator, parser, formatter, 5 codegen targets, action keyword, handler safety, goto type validation, spans on If/BinOp/Perform |
| gust-runtime | 45 | Machine/Supervisor/Envelope traits |
| gust-cli | 48 | all subcommands + doctor + schema; coverage 63% (up from 43%) |
| gust-lsp | 85 | LSP features + pure helpers |
| gust-mcp | 52 | tool integration + `kind` field + `-32700` malformed-JSON + `parse_or_err` helper |
| gust-build | 31 | build-script helper, all 5 targets |
| gust-stdlib | 121 | 6 machines + EngineFailure type |

## What the v0.2 pass shipped

Merged PRs (in merge order): #52 (CI coverage/audit) → #53 (parser expect + error tests) → #54 (WASM/no_std coverage ~97%) → #55 (If/BinOp spans, #46 closed) → #56 (CLI coverage 43→63%) → #57 (public API docs + missing_docs warn) → #58 (rustdoc broken-link fix + CI gate) → #61 (parser pub(crate) + MCP unwrap fixes + Go codegen unreachable!) → #67 (strip `use std::*` at codegen, closes #66) → #62 (workflow-runtime integration guide) → #63 (Perform spans + nested-perform action safety) → #64 (GustWarning::help + parse_or_err + CHANGELOG) → #65 (workflow_engine example uses action/EngineFailure/supervises) → #68 (remove post-codegen strip hack from #65's build.rs).

## Open Issues

- **#60** — VS Code Marketplace publishing (scoped out of 0.2; full recipe in issue body, ready to execute).

## Recent Validator Capabilities

- Name duplication / undefined references
- Goto arity + goto field type compatibility (conservative inference)
- Effect arity + effect return type vs let annotation
- Match exhaustiveness
- Handler fall-through + coverage
- If/else branch termination consistency **(real spans)**
- Binary operator operand compatibility **(real spans)**
- `ctx.field` against from-state
- Handler-safety for `action`: >1 action per path **(now counts nested performs in binop/unary/fieldaccess/fncall)**, action-not-last-side-effect
- Diagnostics emit at the offending `perform` site, not the handler open-brace

## Known Language Constraints

- **Enum variants positional-only.** `Bar(name: String, n: i64)` does not parse; field meanings go in the `.gu` header comment.
- **`use std::*` stripped at codegen.** Both Rust and Go codegens skip imports whose first segment is `std` (#66/#67). Stdlib sources must be compiled alongside dependent machines by the consumer's build pipeline.
- **Expression span precision.** `BinOp` and `Perform` carry spans; `FnCall`, `FieldAccess`, `UnaryOp`, `Path` do not. Not a user-visible issue — diagnostics fall back to enclosing statement span.
- **Validator defers deep type inference** to the host language for unknown types (Phase 10 owns the full checker).

## Public API

`gust-lang`'s parser module is `pub(crate)` as of 0.2; only the crate-root re-exports are public. `#![warn(missing_docs)]` is enabled workspace-wide. ~280 previously-undocumented pub items were documented in PR #57.

## Architecture Notes

- Pipeline: `source.gu → Parser (pest PEG) → AST → Validator → Codegen → .g.rs / .g.go`
- Generated extension: `.g.rs` / `.g.go`
- `perform` is an expression (allows `let x = perform effect(args)`)
- Effect traits: `{Machine}Effects` trait per machine
- **`EffectKind`** on `EffectDecl` distinguishes `effect` (replay-safe) from `action` (not replay-safe). Single field; codegen emits markers; MCP reports `kind` field.
- Goto field mapping: positional zip
- Examples excluded from workspace, tested via `--manifest-path`

## Downstream Consumers

**Corsac** (`D:/Dev/go/corsac/`) is the primary real-world consumer. Uses Gust as the contract language for compiled workflows. Phase 1 Corsac uses node-boundary checkpoints, driving the `effect`/`action` split. Consumes `kind` from MCP `gust_parse` and uses `EngineFailure` from the stdlib.

**v0.2 Corsac-facing deliverables:**
- `docs/src/guides/workflow_runtime.md` — integration guide
- `examples/workflow_engine/` — runnable starter teaching `action`, `EngineFailure`, `supervises`
- `gust-mcp` with `kind` field, `parse_or_err`, `-32700` malformed-JSON handling
