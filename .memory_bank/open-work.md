# Open Work Items

Last updated: 2026-04-21

## v0.2 Status

**Tag-ready.** All merge work done. `CHANGELOG.md` `[Unreleased]` holds the 0.2 body; user rolls to `[0.2.0] - 2026-04-21` when cutting the tag.

## Open PRs
_None._

## Open Issues
- **#60** — Publish VS Code extension to Marketplace (scoped out of 0.2; full recipe in issue body).

## Roadmap Remaining (from ROADMAP.md)

### Phase 1 — Compiler Gaps
1. ~~Expression-level source spans (#46)~~ ✅ Closed by PR #55 (If/BinOp) + PR #63 (Perform).
2. `gust test` subcommand — mock effects, test runner
3. Multi-file type resolution — cross-file `use` declarations

### Phase 2 — LSP
4. Cross-file go-to-definition — LSP follows `use` imports

### Phase 3 — VS Code Extension
5. Bundling / release story — pre-built `gust-lsp` binary in VSIX (partially covered by #60)

### Phase 4 — MCP Server
6. `gust_new` MCP tool — AI-driven machine scaffolding

### Phase 6+ (Long-term, General-Purpose Core)
Long-horizon track: Phases 6–11 covering top-level `fn` declarations, expression-oriented control flow, mutation semantics, contracts, a real type system / effect handlers, and research-track items (SMT, dependent contracts, formal verification). See `ROADMAP.md`.

## Recently shipped — v0.2 tightening pass (2026-04-21)

Merged in order: #52 → #53 → #54 → #55 → #56 → #57 → #58 → #61 → #67 → #62 → #63 → #64 → #65 → #68.

- **CI** — coverage (cargo-llvm-cov + Codecov), cargo-audit, rustdoc broken-link enforcement
- **Parser** — `.expect(GRAMMAR_INVARIANT)` on 80 sites; module `pub(crate)` (semver)
- **Validator** — spans on `Statement::If`, `Expr::BinOp`, `Expr::Perform`; nested-perform traversal for action safety; diagnostics at real perform site
- **Codegen** — WASM/no_std coverage ~97%; `use std::*` stripped (#66 fix); `unreachable!` on wildcard → `"_"`
- **Docs** — `#![warn(missing_docs)]` workspace-wide (~280 items); new `workflow_runtime.md` guide for Corsac
- **MCP** — 3 unwraps → error propagation; `parse_or_err` helper; `-32700` malformed-JSON confirmed + tested
- **Example** — `workflow_engine` uses `action`, `EngineFailure`, `supervises` for Corsac onboarding

## No Known Quality Gaps

All crates tested. Workspace line coverage 85.95%. CHANGELOG up to date. Rustdoc clean under `-D warnings -D rustdoc::broken_intra_doc_links`. Public API documented. Memory bank current.
