# Gust Roadmap

This roadmap has two horizons.

**Near-term (Phases 1–5):** production-readiness for Gust as it exists today — a state machine language. Polish the compiler, LSP, tooling, and AI integration.

**Long-term (Phases 6+):** evolve Gust from a state machine DSL into a general-purpose language. Machines remain a first-class construct, but Gust grows to support free functions, module-level code, contracts, and its own type/effect system. Each phase is independently shippable — no phase blocks on a subsequent one, and Gust-as-DSL remains valid at every step.

Items are ordered by dependency and daily-use impact.

---

## Phase 1 — Compiler Gaps

These are correctness and ergonomics issues in the core toolchain.

- [x] **Exhaustive goto check** — warn when a handler has code paths that don't terminate with a `goto` (fall-through without state transition)
- [x] **Handler coverage check** — warn when a transition is declared but has no corresponding `on` handler
- [x] **Multi-machine diagram** — `gust diagram` now supports `--machine NAME` flag and emits all machines when no flag is given
- [x] **Source span tracking** — replace string-search source location in the validator with actual span data from the parser (fragile on duplicate identifiers)
- [x] **Effect argument arity validation** — `perform` invocations type-checked against effect declaration arity
- [x] **Match exhaustiveness diagnostics** — warn on non-exhaustive matches over known enums; exhaustive matches count as termination
- [x] **Goto field type validation** — check `goto` arguments against target state field types with conservative inference
- [x] **Handler expression type checking** — effect return-type annotations, if/else branch termination consistency, binary operator operand compatibility
- [x] **`action` keyword for non-idempotent operations** — `EffectKind::Action` in AST/codegen/MCP + handler-safety diagnostics for workflow runtimes (#40)
- [x] **`EngineFailure` in stdlib** — typed runtime failure enum for workflow contracts (#40)
- [x] **`gust doctor` subcommand** — environment diagnostics (rustc/cargo/go/project layout/`.gu` freshness)
- [x] **JSON Schema codegen** — `--target schema` / `gust schema` emits JSON Schema from types and machine states
- [x] **Optional tracing codegen** — `RustCodegen::with_tracing(true)` emits `tracing::info!` events guarded by a `tracing` feature flag
- [ ] **Expression-level source spans** — extend span tracking to `Statement::If`, `Statement::Match`, `Statement::Let`, and all `Expr::*` nodes so validator diagnostics don't fall back to line 0, col 0 (#46)
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

## Phase 6 — General-Purpose Core

The fork in the road. After this phase, machines are one construct among many. A `.gu` file can compile and run without declaring a single machine.

Ordering matters here: the first item gates all the others.

- [ ] **Top-level `fn` declarations** — add `fn_decl` as a valid top-level item in `grammar.pest` alongside `use_decl`, `type_decl`, `channel_decl`, `machine_decl`. This is the single grammar change that stops Gust from being a DSL.
- [ ] **Standalone modules** — a `.gu` file with no machines parses, validates, and compiles to a usable library module.
- [ ] **Top-level `const` and `let`** — module-level immutable bindings for constants and shared values.
- [ ] **Expression-bodied functions** — `fn add(a: i32, b: i32) -> i32 = a + b;`. Reduces ceremony for single-expression functions, matches existing `perform` expression semantics.
- [ ] **Block expressions** — `{ ... }` blocks evaluate to their final expression. Enables `let x = { let tmp = compute(); tmp * 2 };` and multi-line expression bodies.
- [ ] **`Option<T>` and `Result<T, E>` as built-in types** — currently expressible as user enums. Built-in forms get first-class pattern syntax, codegen specialization, and standard library support.
- [ ] **`main()` entry point** — when a module defines `fn main()`, codegen emits an executable binary, not just a library.

---

## Phase 7 — Expression-Oriented Flow

Control flow as values. This phase makes Gust feel like an expression-oriented language in the mold of Rust, Kotlin, and F#. Read-at-2-AM syntax pays off here.

- [ ] **`if` as expression** — `let x = if cond { a } else { b };` produces a value. Both branches must type-unify.
- [ ] **`match` as expression (outside machines)** — already a statement inside machines; lift to general use and allow it to produce values.
- [ ] **Inline error matching at call sites** — the distinctive Gust syntax: bind and match in a single construct. `let u = db.find(id) { ok(u) -> u; not_found -> default_user; err(e) -> panic(e) };` — replaces the separate `let` + `match` two-step. Needs explicit grammar design to avoid ambiguity with block bodies.
- [ ] **Error propagation operator** — decide explicitly: support `?`-style short-circuit, or force inline matching everywhere. Implications for readability in deep error paths.
- [ ] **Destructuring patterns in `let` and parameters** — `let { name, age } = user;` and `fn greet({ name }: User) = ...`.
- [ ] **Guards in match arms** — `n if n > 0 => ...` for range and predicate matching.

---

## Phase 8 — Mutation Semantics

Make mutation visible, bounded, and deterministic. This is a semantic change with syntactic consequences, not the other way around. Several decisions here need to be made *before* any code is written.

- [ ] **Immutability by default for `let`** — current `let` inherits the host language's rules. Gust needs its own stance: bindings are immutable unless introduced in a `mut` context.
- [ ] **`mut` blocks with lexical scope** — `mut counter { counter += 1 }`. Outside the block, the binding is frozen again. Enforced by the validator, not just convention.
- [ ] **Decide: aliasing rules inside `mut`** — Rust-style exclusive access (soundness), or permissive with runtime checks (ergonomics). This decision shapes every subsequent concurrency guarantee.
- [ ] **Decide: can values escape a `mut` block?** — can a `mut` block produce a value used outside it? If yes, how is the mutation prevented from leaking? Options: rank-2 types (Haskell ST), relying on Rust's borrow checker through codegen, or forbidding escape entirely.
- [ ] **Interior mutability escape hatch** — explicit opt-in for memoization, lazy fields, and other observationally-pure mutation.
- [ ] **I/O story** — document the rule: does `println` require a `mut` block? If yes, consistent but ceremonial. If no, principled exception needed. Either answer is defensible; silence is not.
- [ ] **Codegen for `mut`** — Rust target maps cleanly (`let mut` inside the block). Go target needs explicit handling since Go has no equivalent concept.

---

## Phase 9 — Contracts

Runtime-checked first. Compile-time proving is a later research track (Phase 11). The goal here is to make contracts part of the language, not a library.

- [ ] **`require` / `ensure` on function signatures** — preconditions and postconditions as part of the declaration, not doc comments. Visible in hover, signature help, generated docs.
- [ ] **Runtime contract generation** — codegen emits assertion guards at the start and end of function bodies. Rust target uses `assert!`; Go target uses `panic` with explicit messages.
- [ ] **`where` clauses on types** — refinement types: `type PositiveInt = i32 where self > 0`. Validated on construction.
- [ ] **Contract-aware serialization** — deserializing a type validates its contracts, not just its shape. Invalid JSON → typed error, not a runtime surprise 20 lines later.
- [ ] **`assume` escape hatch** — for contracts that cannot or should not be checked at a particular call site. Explicit, auditable, greppable.
- [ ] **Documentation from contracts** — `gust doc` surfaces `require`/`ensure` as part of the public API. Contracts are specification, not implementation.
- [ ] **Contract inheritance rules** — what happens when a refined type is used in a struct field? Nested contracts compose or conflict; the rule needs to be explicit.

---

## Phase 10 — Type System & Effects

Extend the existing effect system beyond machines and start owning type-checking instead of delegating to the target language. This is where Gust stops being a transpiler and becomes a compiler with a backend.

- [ ] **Effects on free functions** — `effect` is currently a machine-local concept. Free functions need the same first-class effect declarations.
- [ ] **Effect polymorphism for higher-order functions** — a `map` function that's polymorphic over the callback's effects. Without this, the standard library needs N copies of every combinator (sync/async/fallible/mutating). Koka and Roc are the reference models.
- [ ] **Own type checker** — stop relying on Rust/Go type errors surfacing through generated code. Diagnostics should point at `.gu` source, not generated `.g.rs`.
- [ ] **Type inference** — bidirectional inference for the 80% case; explicit annotations at module boundaries and public API surfaces.
- [ ] **Opaque / branded types** — nominal escape hatch from the structural default. `opaque type UserId = UUID;` makes `UserId` and `OrderId` incompatible even when the underlying representations match.
- [ ] **Trait bounds on generics wired end-to-end** — grammar already supports `T: Bound1 + Bound2`; validator and codegen need to enforce the bound instead of passing it through.
- [ ] **Effect handlers (resumable continuations)** — current `perform` delegates to a host-language trait impl. True effect handlers enable algebraic effects: a caller can catch, transform, or resume an effect mid-flight. Significant codegen work.

---

## Phase 11 — Research Track (long-horizon)

Explicitly framed as research, not a shipping commitment. Do not block other phases on these. Most of these are multi-year investments with uncertain outcomes.

- [ ] **SMT integration (Z3)** — compile-time proving of decidable contract fragments (arithmetic bounds, null-safety, simple invariants). Define the decidable syntactic subset explicitly; everything outside falls back to runtime.
- [ ] **Dependent contracts** — `List<T> where len(self) >= 1`, where the contract depends on a runtime value. Expressive but hard to infer and prove.
- [ ] **Property-based testing from contracts** — generate QuickCheck-style tests automatically from `require`/`ensure`. Low-hanging fruit once contracts exist.
- [ ] **Formal verification mode** — opt-in per module flag; all contracts must be proved statically or compilation fails. Reference: Dafny, F*, SPARK.
- [ ] **Ownership inference** — infer Rust-style borrow relationships from usage patterns without requiring lifetime annotations in `.gu` source. Not a solved problem; research-grade work.
- [ ] **Incremental compilation** — only recheck/regenerate what changed. Becomes critical once the type checker and contract prover get expensive.

---

## Remaining

- [ ] Expression-level source spans (Phase 1, #46) — spans on `Statement::If`, `Statement::Match`, `Statement::Let`, `Expr::*`
- [ ] `gust test` subcommand (Phase 1) — mock effects, test runner
- [ ] Multi-file type resolution (Phase 1) — cross-file `use` declarations
- [ ] Cross-file go-to-definition (Phase 2) — LSP follows `use` imports
- [ ] VS Code bundling (Phase 3) — VSIX with pre-built `gust-lsp` binary
- [ ] `gust_new` MCP tool (Phase 4) — AI-driven machine scaffolding

---

## Done

- [x] Core grammar and parser (`grammar.pest`, `ast.rs`, `parser.rs`)
- [x] Rust codegen (`codegen.rs`) — full target including async, generics, FFI, optional tracing instrumentation
- [x] Go codegen (`codegen_go.rs`) — interfaces, unit effects, async effects
- [x] WASM and no_std codegens
- [x] JSON Schema codegen — types + machine states to JSON Schema
- [x] Formatter (`format.rs`)
- [x] Validator — duplicate names, unreachable states, unknown effects, goto arity + **goto field type validation**, ctx field access, send/spawn targets, typo suggestions, **exhaustive goto check**, **handler coverage**, **effect argument arity**, **match exhaustiveness**, **effect return type checking on let annotations**, **if/else branch termination consistency**, **binary operator operand compatibility**, **handler-safety diagnostics for `action`**
- [x] Source span tracking — top-level nodes carry `Span` captured from pest
- [x] `effect` vs `action` distinction — `EffectKind` on `EffectDecl`, formatter roundtrip, codegen doc markers (Rust rustdoc + Go `//`), MCP `kind` field
- [x] CLI — `build`, `check`, `fmt`, `diagram` (multi-machine), `watch`, `init`, `parse`, **`doctor`**, **`schema`**
- [x] `-> ()` unit type support
- [x] LSP — diagnostics, hover (states, effects, transitions, types), go-to-definition, completions, **formatting, documentSymbol, signatureHelp, rename, references, code actions, inlay hints**
- [x] VS Code extension — syntax highlighting, snippets (10 total), file icon, LSP client, file nesting, **commands (diagram/check/format), status bar**
- [x] MCP server (`gust-mcp`) — 5 tools: check, build, diagram, format, parse (now with `kind` on effects)
- [x] Claude Code plugin — 3 commands, 1 skill, 1 agent, drop-in GUST.md
- [x] Standard library — `CircuitBreaker`, `Retry`, `Saga`, `RateLimiter`, `HealthCheck`, `RequestResponse` machines + `EngineFailure` type (Corsac workflow contract)
- [x] Corsac architecture documentation (`D:/Dev/go/corsac/docs/`)
