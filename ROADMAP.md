# Gust Roadmap

This roadmap covers the remaining work to make Gust production-ready across the compiler, LSP, VS Code extension, and AI tooling. Items are ordered by dependency and daily-use impact.

---

## Phase 1 — Compiler Gaps

These are correctness and ergonomics issues in the core toolchain.

- [x] **Exhaustive goto check** — warn when a handler has code paths that don't terminate with a `goto` (fall-through without state transition)
- [x] **Handler coverage check** — warn when a transition is declared but has no corresponding `on` handler
- [x] **Multi-machine diagram** — `gust diagram` now supports `--machine NAME` flag and emits all machines when no flag is given
- [x] **Source span tracking** — replace string-search source location in the validator with actual span data from the parser (fragile on duplicate identifiers)
- [ ] **`gust test` subcommand** — unit-test machines with mock effects; define test cases in `.gu` or a companion `.gu.test` file
- [ ] **Multi-file type resolution** — allow `use` declarations to pull types from other `.gu` files in the same project

---

## Phase 2 — LSP

Highest daily-use value. Start with the three features editors rely on most.

- [x] **`textDocument/formatting`** — format via LSP so editors can format-on-save without shelling out to `gust fmt`
- [x] **`textDocument/documentSymbol`** — populate the outline panel and breadcrumbs with states, transitions, effects, and types
- [x] **`textDocument/signatureHelp`** — show effect parameter hints when cursor is inside `perform foo(`
- [x] **Hover on transitions and types** — extended to transition declarations and struct/enum types
- [x] **`textDocument/rename`** — rename a state, effect, or type across the current file
- [x] **`textDocument/references`** — find all usages of a state or effect
- [x] **Code actions** — "Add missing handler for transition X" with correct context type stub
- [x] **Inlay hints** — show effect return types inline on `let` bindings from `perform` calls
- [ ] **Cross-file go-to-definition** — resolve `use` imports to definitions in other `.gu` files

---

## Phase 3 — VS Code Extension

- [x] **`Gust: Show State Diagram` command** — webview panel that renders `gust diagram` output as a live Mermaid diagram; updates on save
- [x] **`Gust: Format Document` command** — explicit palette command (uses LSP formatter)
- [x] **`Gust: Check File` command** — run `gust check` and surface results in the Problems panel
- [x] **Status bar item** — shows `$(circuit-board) Gust`, visible for `.gu` files, click opens diagram
- [x] **Expanded snippets**:
  - `machine` — full machine scaffold (states, transitions, effects, handlers)
  - `effect` — effect declaration with return type
  - `on` — handler stub with ctx parameter
  - `node` — complete Corsac node pattern (Idle → Configured → Executing → Done | Failed)
  - `type` — struct declaration
  - `async on` — async handler stub
  - `match` — match expression scaffold
- [ ] **Bundling / release story** — pre-built `gust-lsp` binary bundled in the VSIX or download-on-install; stop relying on `target/debug`

---

## Phase 4 — MCP Server (`gust-mcp`)

Expose the Gust compiler as MCP tools so any AI assistant with MCP support can call the compiler directly.

- [x] **`gust_check(file)`** — run `gust check` and return structured diagnostics as JSON (errors + warnings with line/col)
- [x] **`gust_build(file, target)`** — compile a `.gu` file to the specified target (rust/go/wasm/nostd/ffi) and return the generated source
- [x] **`gust_diagram(file, machine?)`** — return Mermaid state diagram string for one or all machines
- [x] **`gust_format(file)`** — return formatted source without writing to disk
- [x] **`gust_parse(file)`** — return the AST as structured JSON (hand-written serializer)
- [ ] **`gust_new(name, description)`** — scaffold a new `.gu` machine from a natural-language description using Claude

---

## Phase 5 — Claude Code Plugin (`gust` plugin)

- [x] **Plugin scaffold** — `plugin.json`, commands, skills, agent at `D:/Dev/tools/gust-plugin/`
- [x] **`/gust-new` command** — generate a new `.gu` machine from a description
- [x] **`/gust-check` command** — run check on current file and display diagnostics
- [x] **`/gust-diagram` command** — show state diagram in terminal
- [x] **`gust-designer` agent** — understands Corsac node patterns, effect conventions, and IPC protocol; generates valid `.gu` contracts from natural language
- [x] **Skill: Gust state machine author** — teaches Claude to write idiomatic Gust: state threading, unit effects, ctx fields, Corsac patterns, review checklist
- [x] **Project context injection** — drop-in `GUST.md` for `.claude/` directories

---

## Remaining

- [x] Source span tracking (Phase 1) — parser spans instead of string search
- [ ] `gust test` subcommand (Phase 1) — mock effects, test runner
- [ ] Multi-file type resolution (Phase 1) — cross-file `use` declarations
- [ ] Cross-file go-to-definition (Phase 2) — LSP follows `use` imports
- [ ] VS Code bundling (Phase 3) — VSIX with pre-built `gust-lsp` binary
- [ ] `gust_new` MCP tool (Phase 4) — AI-driven machine scaffolding

---

## Done

- [x] Core grammar and parser (`grammar.pest`, `ast.rs`, `parser.rs`)
- [x] Rust codegen (`codegen.rs`) — full target including async, generics, FFI
- [x] Go codegen (`codegen_go.rs`) — interfaces, unit effects, async effects
- [x] WASM and no_std codegens
- [x] Formatter (`format.rs`)
- [x] Validator — duplicate names, unreachable states, unknown effects, goto arity, ctx field access, send/spawn targets, typo suggestions, **exhaustive goto check, handler coverage**
- [x] CLI — `build`, `check`, `fmt`, `diagram` (multi-machine), `watch`, `init`, `parse`
- [x] `-> ()` unit type support
- [x] LSP — diagnostics, hover (states, effects, transitions, types), go-to-definition, completions, **formatting, documentSymbol, signatureHelp, rename, references, code actions, inlay hints**
- [x] VS Code extension — syntax highlighting, snippets (10 total), file icon, LSP client, file nesting, **commands (diagram/check/format), status bar**
- [x] MCP server (`gust-mcp`) — 5 tools: check, build, diagram, format, parse
- [x] Claude Code plugin — 3 commands, 1 skill, 1 agent, drop-in GUST.md
- [x] Corsac architecture documentation (`D:/Dev/go/corsac/docs/`)
