# Gust Language Roadmap

## Vision

Gust is a type-safe state machine DSL that transpiles to Rust and Go for building stateful services. It eliminates the boilerplate of hand-writing state machines, effect tracking, and serialization boundaries — generating correct, idiomatic code from a concise DSL.

**Core thesis**: Most production bugs aren't algorithm bugs — they're state management bugs, unhandled edge cases at service boundaries, and hidden side effects. Gust makes those structurally impossible.

**Positioning**: Gust is a Rust/Go companion, not a replacement. Drop `.gu` files into an existing workspace. Gust generates the state machine layer; you write the effect implementations that connect to the real world.

**Universal service contract**: Same `.gu` file generates Rust or Go. Teams agree on state machines regardless of runtime choice.

---

## Current State (v0.1.0)

**Status: Released**

`v0.1.0` delivers end-to-end transpilation: `.gu` source in, compilable Rust/Go out, with tooling and runtime support.

### What shipped
- PEG grammar for machine declarations (pest-based)
- Full AST: machines, states, transitions, effects, handlers, expressions
- Parser with `perform` as both statement and expression
- Rust code generation (structs, state enums, transition methods, effect traits, handler bodies)
- Go code generation (iota enums, data structs, interfaces, PascalCase, nil-clearing, JSON helpers)
- CLI with `parse` and `build` commands, multi-target support (`--target rust|go`)
- `.g.rs` / `.g.go` output convention
- Runtime library with Machine/Supervisor traits and Envelope messaging

---

## Phase 1 - Make It Real

**Status: Complete** (commit `35e41c5`, spec: `docs/specs/SPEC-PHASE1.md`)

**Goal**: Use Gust in a real Rust project. Generated code compiles and runs.

### Cargo integration
- [x] `build.rs` integration via `gust-build` crate: auto-run gust compiler on `.gu` files during `cargo build`
- [x] Incremental builds (mtime-based), `cargo:rerun-if-changed` directives
- [x] `gust watch` — re-generate `.g.rs`/`.g.go` on file save (100ms debounce, deletion handling)

### Import resolution
- [x] `use` declarations in `.gu` resolve to Rust modules
- [x] Generated code imports types from the host crate
- [x] Handle Rust path mapping (e.g., `use crate::models::Order`)
- [x] Go import mapping (`::` → `/`)

### Async support
- [x] `async` transition handlers (named `async_modifier` rule for pest detection)
- [x] `async fn` effect trait methods
- [x] Tokio runtime integration (Rust), `context.Context` (Go)
- [x] Generated code uses `async/await` for effect calls, `.await` on async perform

### Type system improvements
- [x] Enum types in Gust (`TypeDecl` as enum with Struct/Enum variants)
- [x] Option/Result handling in handler bodies
- [x] Pattern matching in handlers (`match` statement with `Pattern::Variant`)
- [x] Tuple types (Rust tuples, Go anonymous structs)

---

## Phase 2 - Developer Experience

**Status: Complete** (commit `5f7632b`, spec: `docs/specs/SPEC-PHASE2.md`)

**Goal**: Make writing Gust feel productive, not like fighting a tool.

### VS Code extension (`editors/vscode/`)
- [x] TextMate grammar for syntax highlighting (.gu files) with categorized keyword scopes
- [x] File icon for `.gu` files
- [x] Snippet support (machine, state, transition, effect, on, async effect templates)
- [x] Auto-collapse `.g.rs`/`.g.go` files in explorer (file nesting)
- [x] Language configuration (brackets, indentation rules, comment toggling)

### Language Server (LSP) (`gust-lsp/`)
- [x] Syntax error diagnostics with line/column info (integrated with validator)
- [x] Go-to-definition within `.gu` files (states, effects, transitions)
- [x] Hover info (state fields with types, effect signatures)
- [x] Autocomplete for state names in `goto`, effect names in `perform`
- [x] Configurable via `gust.lsp.path` VS Code setting

### Error messages (`gust-lang/src/error.rs`)
- [x] Human-friendly parser errors with context (cargo-style colored output, NO_COLOR support)
- [x] Suggestions via help field on GustError/GustWarning
- [x] Transition validity checking before codegen
- [x] Detect unreachable states and dead transitions

### Validator (`gust-lang/src/validator.rs`)
- [x] Duplicate state detection
- [x] Undeclared target states, effects, channels, machines
- [x] Goto argument count validation
- [x] Unreachable states and unused effects
- [x] SourceLocator for real line/column positions

### Tooling
- [x] `gust init` — scaffold a new Gust-enabled Rust project
- [x] `gust fmt` — format `.gu` files (4-space indent, channels, timeouts)
- [x] `gust check` — validate without generating (fast feedback loop)
- [x] `gust diagram` — generate state machine visualization (mermaid/dot)

---

## Phase 3 - Structured Concurrency

**Status: Complete** (Rust: commit `5f2ef6a`, Go parity: commit `8a3fd0b`, spec: `docs/specs/SPEC-PHASE3.md`)

**Goal**: Multiple machines communicating with supervision. Local structured concurrency (transport layer deferred).

### Inter-machine channels
- [x] Typed message passing between machines (broadcast/mpsc)
- [x] Channel declarations in `.gu` syntax with capacity and mode
- [x] Generated Rust uses tokio channels, Go uses buffered channels
- [x] Compile-time message type checking (validator enforces undeclared channels)

### Supervision trees
- [x] `supervises` keyword for machine relationships
- [x] Supervision strategies (OneForOne, OneForAll, RestForOne) in runtime
- [x] SupervisorRuntime with spawn_named(), join_next(), restart_scope()
- [x] ChildHandle for lifecycle management

### Lifecycle management
- [x] `spawn` as a language primitive (validator enforces undeclared machines)
- [x] `send` statement for channel message passing
- [x] Timeout handling on transitions (duration specs: ms/s/m/h units)
- [x] Rust: `tokio::time::timeout()` wrapping, Go: `context.WithTimeout`

### Cross-boundary serialization
- [x] Machines communicate in-process via channels
- [ ] Network boundary transport (deferred — intentionally scoped to local-only)

---

## Phase 4 - Ecosystem

**Status: Complete** (commits `ce944aa` → `64efc41`, spec: `docs/specs/SPEC-PHASE4.md`)

**Goal**: Other people can use Gust and contribute to it.

### Standard library (`gust-stdlib/`)
- [x] Request/response machine (generic)
- [x] Circuit breaker machine (generic, with half-open state)
- [x] Saga machine (generic, multi-step with per-step compensation)
- [x] Retry machine (generic, with exponential backoff and jitter)
- [x] Rate limiter machine (generic)
- [x] Health check / heartbeat machine (generic)

### Machine generics
- [x] `machine Foo<T: Clone>` syntax in grammar
- [x] Generic parameters with trait bounds in AST
- [x] Parser support for generic_params
- [x] Rust codegen with `<T: Clone>` trait bounds
- [x] Go codegen with `[T any]` generics (Go 1.18+)

### Documentation (`docs/`)
- [x] mdBook scaffold (book.toml, SUMMARY.md, 42 pages)
- [x] CI workflow validates Gust snippets parse
- [ ] Page content (placeholder — being filled by Codex)

### Community
- [x] GitHub template repository scaffold
- [x] Example project scaffolds (microservice, event processor, workflow engine, IoT, chat, CQRS)
- [x] Package registry design document
- [ ] Example content (placeholder — being filled by Codex)

### Compilation targets
- [x] WASM target (`codegen_wasm.rs`): `#[wasm_bindgen]` attrs, `future_to_promise`, JS effect adapters, `JsValue` fallback types
- [x] `no_std` target (`codegen_nostd.rs`): `heapless::String<64>`, `heapless::Vec<T, 16>`, `extern crate alloc`, no serde
- [x] C FFI target (`codegen_ffi.rs`): `#[repr(C)]`, handle-based API, null-safety (`-1`/`-2` return codes), `.h` header generation

---

## Test Coverage

Workspace tests, checks, and clippy are required clean before release tags.

Current suites include:

- `gust-lang` unit and integration tests:
  - language semantics
  - diagnostics and validation
  - Rust and Go concurrency/codegen
  - generics and backend targets
  - docs snippet parsing
  - parser property tests
  - import resolution
- `gust-build` integration tests
- `gust-runtime` supervision/runtime tests
- `gust-stdlib` parse/validate coverage

## Known Limitations (v0.1.0)

- `gust init` auto-detects parent Cargo workspaces and generates projects with `[workspace]` when needed.
- Projects scaffolded before this behavior may still require a manual `[workspace]` table in `Cargo.toml`.
- Inter-machine communication is currently local in-process channels only. Network transport remains deferred.

---

## Design Principles

These guide every decision:

1. **Gust is Rust/Go** — Generated code is idiomatic. No runtime magic. Inspect, debug, and extend the output like any native code.
2. **State machines are the primitive** — Everything is a machine. Concurrency, error handling, lifecycle management.
3. **Effects are explicit** — No hidden side effects. If a function talks to a database, the type system says so.
4. **Boring generated code** — The output should be obvious, readable code that a tired engineer at 2am can follow.
5. **Incremental adoption** — Drop a `.gu` file into an existing project. No rewrite required.
6. **Correctness by construction** — Invalid states and transitions are compile errors, not runtime surprises.
7. **Universal service contract** — Same `.gu` file, different runtime. Teams agree on state machines, not languages.

---

## What's Next

All four roadmap phases are implemented. Remaining work:

### Content (in progress)
- [ ] Fill out mdBook documentation pages (42 pages, currently placeholders)
- [ ] Write substantive example projects (6 scaffolds exist)

### Hardening (future)
- [ ] Real-world dogfooding — build a service with Gust and fix what breaks
- [ ] Generated code compilation testing (Rust: `cargo check`, Go: `go vet`)
- [ ] Property-based testing for parser/codegen roundtrips
- [ ] Benchmark codegen performance on large `.gu` files
- [ ] WASM target end-to-end testing (actual `wasm-pack build`)
- [ ] no_std target testing on embedded hardware
- [ ] C FFI target testing with C consumer program

### Polish (future)
- [ ] `gust-lsp` publish to crates.io
- [ ] VS Code extension publish to marketplace
- [ ] Cross-boundary serialization (Phase 3 deferred item)
- [ ] Network transport layer for inter-machine communication
- [ ] Comment preservation in formatter
