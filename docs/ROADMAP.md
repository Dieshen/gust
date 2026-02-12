# Gust Language Roadmap

## Vision

Gust is a transpile-to-Rust programming language for building type-safe stateful services. It eliminates the boilerplate of hand-writing state machines, effect tracking, and serialization boundaries — generating correct, idiomatic Rust from a concise DSL.

**Core thesis**: Most production bugs aren't algorithm bugs — they're state management bugs, unhandled edge cases at service boundaries, and hidden side effects. Gust makes those structurally impossible.

**Positioning**: Gust is a Rust companion, not a replacement. Drop `.gu` files into an existing Rust workspace. Gust generates the state machine layer; you write the effect implementations that connect to the real world.

---

## Current State (v0.1 - POC)

**Status: Complete**

The POC demonstrates end-to-end transpilation: `.gu` source in, compilable Rust out.

### What works
- PEG grammar for machine declarations (pest-based)
- Full AST: machines, states, transitions, effects, handlers, expressions
- Parser with `perform` as both statement and expression
- Code generation:
  - Type declarations -> Rust structs with serde derives
  - State enums with typed variants
  - Machine structs with constructors
  - Transition methods with from-state enforcement
  - State field destructuring in match arms
  - Handler body codegen (let, if/else, goto, perform, return)
  - Expression codegen (field access, function calls, binary/unary ops, literals)
  - Effect trait generation with `&self` methods
  - Effect parameter injection into transition methods
- CLI with `parse` (AST debug) and `build` (Rust codegen) commands
- `.g.rs` output convention (generated files alongside `.gu` source)
- Runtime library with Machine/Supervisor traits and Envelope messaging

### Architecture
```
source.gu -> Lexer -> Parser -> AST -> RustCodegen -> .g.rs
                (pest)   (parser.rs)     (codegen.rs)
```

### File convention
```
src/
  order_processor.gu       # Gust source (you write this)
  order_processor.g.rs     # Generated Rust (compiler output, don't edit)
  effects.rs               # Effect implementations (you write this)
```

---

## Phase 1 - Make It Real

**Goal**: Use Gust in a real Rust project. Generated code compiles and runs.

### Cargo integration
- [ ] `build.rs` integration: auto-run gust compiler on `.gu` files during `cargo build`
- [ ] Or `cargo-gust` subcommand plugin
- [ ] `gust watch` — re-generate `.g.rs` on file save

### Import resolution
- [ ] `use` declarations in `.gu` resolve to Rust modules
- [ ] Generated code imports types from the host crate
- [ ] Handle Rust path mapping (e.g., `use crate::models::Order`)

### Async support
- [ ] `async` transition handlers (most real services are async)
- [ ] `async fn` effect trait methods
- [ ] Tokio runtime integration
- [ ] Generated code uses `async/await` for effect calls

### Type system improvements
- [ ] Enum types in Gust (not just structs)
- [ ] Option/Result handling in handler bodies
- [ ] Pattern matching in handlers
- [ ] Tuple types

---

## Phase 2 - Developer Experience

**Goal**: Make writing Gust feel productive, not like fighting a tool.

### VS Code extension
- [ ] TextMate grammar for syntax highlighting (.gu files)
- [ ] File icon for `.gu` files
- [ ] Snippet support (machine, state, transition templates)
- [ ] Auto-collapse `.g.rs` files in explorer

### Language Server (LSP)
- [ ] Syntax error diagnostics with line/column info
- [ ] Go-to-definition within `.gu` files
- [ ] Hover info (state fields, transition targets)
- [ ] Autocomplete for state names in `goto`
- [ ] Autocomplete for effect names in `perform`

### Error messages
- [ ] Human-friendly parser errors with context
- [ ] Suggestions for common mistakes ("did you mean...?")
- [ ] Transition validity checking before codegen
- [ ] Detect unreachable states and dead transitions

### Tooling
- [ ] `gust init` — scaffold a new Gust-enabled Rust project
- [ ] `gust fmt` — format `.gu` files
- [ ] `gust check` — validate without generating (fast feedback loop)
- [ ] `gust diagram` — generate state machine visualization (mermaid/dot)

---

## Phase 3 - Structured Concurrency

**Goal**: Multiple machines communicating with supervision. The full Erlang/OTP promise with static types.

### Inter-machine channels
- [ ] Typed message passing between machines
- [ ] Channel declarations in `.gu` syntax
- [ ] Generated Rust uses tokio channels
- [ ] Compile-time message type checking

### Supervision trees
- [ ] `supervises` keyword for machine relationships
- [ ] Supervision strategies (one-for-one, one-for-all, rest-for-one)
- [ ] Automatic restart on failure
- [ ] Escalation policies

### Lifecycle management
- [ ] `spawn` as a language primitive
- [ ] Graceful shutdown propagation
- [ ] Timeout handling on transitions
- [ ] Cancellation tokens

### Cross-boundary serialization
- [ ] Machines can run in-process or across network boundaries
- [ ] Same `.gu` code, different deployment topology
- [ ] Auto-generated protobuf/JSON serialization for cross-process messages

---

## Phase 4 - Ecosystem

**Goal**: Other people can use Gust and contribute to it.

### Standard library
- [ ] Common machine patterns: request/response, saga, circuit breaker
- [ ] Retry policies as composable machines
- [ ] Rate limiter machine
- [ ] Health check / heartbeat patterns

### Documentation
- [ ] Language reference
- [ ] Tutorial: "Your first Gust service"
- [ ] Guide: "Migrating a Rust state machine to Gust"
- [ ] Cookbook: common patterns

### Community
- [ ] Package registry for reusable machine definitions
- [ ] GitHub template repository
- [ ] Example projects (microservice, event processor, workflow engine)

### Targets
- [ ] WASM compilation target (Gust -> Rust -> WASM)
- [ ] `no_std` support for embedded state machines
- [ ] C FFI generation for cross-language interop

---

## Design Principles

These guide every decision:

1. **Gust is Rust** — Generated code is idiomatic Rust. No runtime magic. Inspect, debug, and extend the output like any Rust code.
2. **State machines are the primitive** — Everything is a machine. Concurrency, error handling, lifecycle management.
3. **Effects are explicit** — No hidden side effects. If a function talks to a database, the type system says so.
4. **Boring generated code** — The output should be obvious, readable Rust that a tired engineer at 2am can follow.
5. **Incremental adoption** — Drop a `.gu` file into an existing Rust project. No rewrite required.
6. **Correctness by construction** — Invalid states and transitions are compile errors, not runtime surprises.
