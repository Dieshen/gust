# Gust Language - Session State Document

**Last Updated**: 2026-02-13
**Version**: v0.1.0 (All Phases Complete: v0.1-v0.4)
**GitHub**: https://github.com/Dieshen/gust (private, owner: Dieshen)

This document captures the complete state of the Gust programming language project for session continuity. A fresh Claude session should read this document to immediately understand the full project context.

---

## 1. Project Overview

### What is Gust?

Gust is a type-safe state machine language that transpiles to Rust and Go. It's designed to eliminate boilerplate in production services where state management bugs are the primary failure mode.

**Core Value Proposition**:
- Write a state machine once in 30 lines of `.gu` code
- Get 300+ lines of correct, idiomatic Rust or Go
- Same source file targets multiple languages
- No invalid states, no unhandled transitions, no hidden side effects

**Positioning**: Gust is a Rust/Go companion, not a replacement. You drop `.gu` files into existing projects. Gust generates the state machine layer; you write the effect implementations that connect to the real world.

**Target Users**:
- Enterprise developers building stateful services
- Teams who want language-agnostic service contracts
- Developers tired of hand-writing state machines with boilerplate

**Current Status**: v0.1 POC complete, plus all four roadmap phases (Phase 1-4) are implemented and tested. All 27 tests passing. Zero clippy warnings. Production-ready foundation complete.

### Core Concepts

1. **Algebraic State Machines**: Define states and transitions declaratively. The compiler enforces that only valid transitions can occur.

2. **Effect Tracking**: Side effects (IO, network, database) are declared as effects. You implement the effects, Gust generates the wiring.

3. **Auto Serialization**: Rust output derives `Serialize`/`Deserialize`. Go output gets `json` struct tags. Cross-service boundaries are type-checked.

4. **Multi-Target**: Same `.gu` source compiles to idiomatic Rust or Go. Teams don't have to agree on a runtime to agree on state machine definitions.

### Why This Approach?

**Why transpile instead of a custom runtime?**
- No runtime overhead or learning curve
- Generated code is debuggable, familiar Rust/Go
- Zero-cost abstractions (in Rust)
- Integrates with existing tooling (cargo, go build)

**Why .g.rs/.g.go convention?**
- Inspired by C# source generators (Brock's background)
- Clearly marks generated code (don't edit)
- Lives alongside source for easy inspection
- Familiar to enterprise developers

**Why effects as traits/interfaces?**
- Forces explicit dependency injection
- Makes side effects visible in function signatures
- Testable (mock the effects trait)
- Idiomatic in both Rust and Go

---

## 2. Architecture

### Compilation Pipeline

```
source.gu → Lexer → Parser → AST → RustCodegen → .g.rs
              (pest)  (parser.rs)      (codegen.rs)
                                  → GoCodegen   → .g.go
                                     (codegen_go.rs)
```

**Stages**:
1. **Lexer/Parser**: pest PEG grammar defines syntax
2. **AST**: Strongly-typed intermediate representation
3. **Codegen**: AST → target language source code
4. **Output**: Compilable Rust/Go code alongside source

### Workspace Structure

```
D:\Projects\gust\
├── Cargo.toml                        # Workspace root
├── README.md                         # Project overview
├── docs/
│   ├── ROADMAP.md                   # Phased development plan (all phases complete)
│   ├── ARCHITECTURE.md              # Compiler architecture doc
│   ├── specs/
│   │   ├── SPEC-PHASE1.md          # Phase 1 spec (2,365 lines)
│   │   ├── SPEC-PHASE2.md          # Phase 2 spec (2,909 lines)
│   │   ├── SPEC-PHASE3.md          # Phase 3 spec (1,946 lines)
│   │   └── SPEC-PHASE4.md          # Phase 4 spec (3,561 lines)
│   ├── guide/
│   │   ├── getting-started.md      # Tutorial (placeholder - Codex filling)
│   │   ├── language-reference.md   # Language reference (placeholder - Codex filling)
│   │   └── cookbook.md             # Common patterns (placeholder - Codex filling)
│   └── api/
│       └── runtime.md               # Runtime API docs (placeholder - Codex filling)
├── examples/
│   ├── order_processor.gu           # Example Gust program
│   ├── order_processor.g.rs         # Generated Rust output
│   ├── basic/                       # Example projects (scaffolds - Codex filling)
│   ├── intermediate/
│   └── advanced/
├── gust-lang/                       # Core compiler library
│   ├── Cargo.toml                   # Dependencies: pest, pest_derive, thiserror, colored
│   └── src/
│       ├── lib.rs                   # Public API
│       ├── grammar.pest             # PEG grammar (expanded with async, channels, generics, enums, match)
│       ├── ast.rs                   # AST node definitions (TypeDecl enum, async flags, channels, enums)
│       ├── parser.rs                # pest → AST conversion (async via Rule::async_modifier)
│       ├── error.rs                 # GustError/GustWarning with SourceLocator, cargo-style colored output
│       ├── validator.rs             # Validates states, effects, channels, machines, goto arity, unreachable states
│       ├── format.rs                # Gust formatter (4-space indent)
│       ├── codegen.rs               # AST → Rust codegen (async/await, effects context threading, enums, match, channels, timeouts, generics)
│       ├── codegen_go.rs            # AST → Go codegen (context.Context, :: → /, string-backed enums, switch, channels, timeouts, generics)
│       ├── codegen_wasm.rs          # AST → WASM target (#[wasm_bindgen], future_to_promise, JsValue fallback)
│       ├── codegen_nostd.rs         # AST → no_std target (heapless types, extern alloc)
│       └── codegen_ffi.rs           # AST → C FFI target (#[repr(C)], handle API, null-safety, .h header)
├── gust-runtime/                    # Runtime support library
│   ├── Cargo.toml                   # Dependencies: serde, serde_json, thiserror, tokio
│   └── src/
│       └── lib.rs                   # Machine/Supervisor traits, Envelope, ChildHandle, SupervisorRuntime
├── gust-cli/                        # CLI binary
│   ├── Cargo.toml                   # Dependencies: gust-lang, clap, notify
│   └── src/
│       └── main.rs                  # Commands: build, watch, parse, init, check, fmt, diagram (100ms debounce)
├── gust-build/                      # Build.rs integration crate
│   ├── Cargo.toml                   # Dependencies: gust-lang
│   └── src/
│       └── lib.rs                   # Target enum (Rust/Go/Wasm/NoStd/Ffi), incremental builds
├── gust-lsp/                        # Language server
│   ├── Cargo.toml                   # Dependencies: tower-lsp, gust-lang, tokio
│   └── src/
│       ├── main.rs                  # LSP server entry point
│       └── handlers.rs              # Diagnostics, go-to-def, hover, completion
├── gust-stdlib/                     # Standard library machines
│   ├── request_response.gu          # Generic request/response pattern
│   ├── circuit_breaker.gu           # Generic with half-open state
│   ├── saga.gu                      # Multi-step with per-step compensation
│   ├── retry.gu                     # Exponential backoff with jitter
│   ├── rate_limiter.gu              # Token bucket pattern
│   └── health_check.gu              # Heartbeat pattern
├── editors/
│   └── vscode/                      # VS Code extension
│       ├── package.json             # Extension manifest
│       ├── syntaxes/
│       │   └── gust.tmLanguage.json # TextMate grammar
│       ├── snippets/
│       │   └── gust.json            # Code snippets
│       └── language-configuration.json # Brackets, comments, etc.
└── memory_bank/
    └── SESSION_STATE.md             # This document
```

### Crate Roles

| Crate | Purpose | Key Files |
|-------|---------|-----------|
| `gust-lang` | Core compiler: parser, AST, codegen, validation, formatting | grammar.pest, ast.rs, parser.rs, error.rs, validator.rs, format.rs, codegen.rs, codegen_go.rs, codegen_wasm.rs, codegen_nostd.rs, codegen_ffi.rs |
| `gust-runtime` | Runtime traits imported by generated code | lib.rs (Machine, Supervisor, Envelope, ChildHandle, SupervisorRuntime with RestartStrategy) |
| `gust-cli` | CLI tool for building/parsing/watching/formatting | main.rs (build, watch, parse, init, check, fmt, diagram commands) |
| `gust-build` | Build.rs integration for Cargo projects | lib.rs (Target enum, incremental builds) |
| `gust-lsp` | Language server for editor integration | main.rs, handlers.rs (tower-lsp, diagnostics, go-to-def, hover, completion) |
| `gust-stdlib` | Standard library of reusable state machine patterns | 6 generic .gu machines (request_response, circuit_breaker, saga, retry, rate_limiter, health_check) |

---

## 3. Grammar & AST

### Grammar Rules Summary (grammar.pest)

**Top-level structure**:
- `program` → `(use_decl | type_decl | machine_decl)*`
- `use_decl` → `use` path segments
- `type_decl` → struct-like type OR enum with variants (TypeDecl::Struct/Enum)
- `enum_decl` → enum name with variants (each variant has optional payload types)
- `machine_decl` → machine name + annotations + body

**Machine annotations**:
- `sends` → channels this machine sends to
- `receives` → channels this machine receives from
- `supervises` → child machines this supervises

**Machine body items**:
- `state_decl` → state name + optional fields
- `transition_decl` → name : from_state -> target_states
- `effect_decl` → effect name + params + return type (with `async_modifier`)
- `on_handler` → handler for a transition with params and body (with `async_modifier`)
- `channel_decl` → channel with capacity/mode (broadcast/mpsc)
- `generic_params` → generic type params with trait bounds

**Statements**:
- `let_stmt` → let bindings with optional type annotation
- `return_stmt` → return expression
- `if_stmt` → if/else with blocks
- `transition_stmt` → `goto` state with args
- `effect_stmt` → `perform` effect with args (statement form)
- `match_stmt` → match expression with Pattern::Variant bindings
- `send_stmt` → send to channel
- `spawn_stmt` → spawn supervised child
- `expr_stmt` → expression followed by semicolon

**Expressions** (precedence chain):
```
expr → or_expr → and_expr → cmp_expr → add_expr → mul_expr → unary_expr → primary
```

**Primary expressions**:
- Literals (int, float, string, bool)
- `perform` effect (args) → **expression form** (key design choice)
- Field access (ident.field)
- Function calls
- Identifiers
- Parenthesized expressions

**Key Design Decisions**:
- `perform` is both a statement and an expression. This allows `let x = perform effect(args)` — effects return values.
- `async_modifier` must be a NAMED rule in pest (not a literal string) so it's visible in Rule:: enum for detection.

### AST Node Types (ast.rs)

**Top-level**:
- `Program { uses, types, machines }`
- `UsePath { segments }`
- `TypeDecl` → enum with `Struct { name, fields }` and `Enum { name, variants }`
  - Helper methods: `name()`, `fields()` for consistent access
- `EnumVariant { name, payload: Vec<TypeExpr> }`
- `MachineDecl { name, states, transitions, handlers, effects, channels, supervision, generic_params, annotations }`

**Machine components**:
- `StateDecl { name, fields }` → state with optional typed fields
- `TransitionDecl { name, from, targets }` → valid state transition
- `EffectDecl { name, params, return_type, is_async }` → tracked side effect signature with async flag
- `OnHandler { transition_name, params, return_type, is_async, body }` → handler implementation with async flag
- `ChannelDecl { name, ty, capacity, mode }` → channel declaration (broadcast/mpsc)
- `SupervisionSpec { strategy, children }` → supervision tree specification
- `GenericParam { name, bounds }` → generic type parameter with trait bounds

**Statements**:
- `Let { name, ty, value }` → variable binding
- `Return(Expr)` → return statement
- `If { condition, then_block, else_block }` → conditional
- `Goto { state, args }` → state transition
- `Perform { effect, args }` → effect invocation (statement form)
- `Match { expr, arms }` → pattern matching on enums
- `Send { channel, value }` → send to channel
- `Spawn { child, args }` → spawn supervised child
- `Expr(Expr)` → expression statement

**Expressions**:
- `IntLit(i64)`, `FloatLit(f64)`, `StringLit(String)`, `BoolLit(bool)`
- `Ident(String)` → identifier
- `FieldAccess(Box<Expr>, String)` → base.field
- `FnCall(String, Vec<Expr>)` → function call
- `BinOp(Box<Expr>, BinOp, Box<Expr>)` → binary operation
- `UnaryOp(UnaryOp, Box<Expr>)` → unary operation
- `Perform(String, Vec<Expr>)` → effect invocation (expression form)

**Type Expressions**:
- `Simple(String)` → simple type name
- `Generic(String, Vec<TypeExpr>)` → generic type with args
- `Tuple(Vec<TypeExpr>)` → tuple type

**Patterns** (for match statements):
- `Pattern::Variant { enum_name, variant_name, bindings }` → enum variant pattern with bindings

---

## 4. Parser (parser.rs)

### How It Works

The parser converts pest `Pair` nodes into AST types. Each grammar rule has a corresponding `parse_*` function.

**Entry point**: `parse_program(source: &str) -> Result<Program, String>`

**Key parsing functions**:
- `parse_machine_decl` → extracts states, transitions, effects, handlers, channels, supervision, generics
- `parse_state_decl` → state name + optional field list
- `parse_transition_decl` → transition name, from-state, target states
- `parse_effect_decl` → effect signature with async detection via Rule::async_modifier
- `parse_on_handler` → handler params, return type, body with async detection via Rule::async_modifier
- `parse_channel_decl` → channel with capacity/mode
- `parse_generic_params` → generic params with trait bounds
- `parse_match_stmt` → match expression with Pattern::Variant
- `parse_program_with_errors` → returns GustError for better error reporting

**Expression parsing** (follows precedence chain):
- `parse_expr` → `parse_or_expr` → `parse_and_expr` → `parse_cmp_expr` → `parse_add_expr` → `parse_mul_expr` → `parse_unary_expr` → `parse_primary`

**Primary expression parsing**:
- Literals → direct conversion
- `perform_expr` → extracts effect name + args → `Expr::Perform`
- `fn_call` → extracts function name + args → `Expr::FnCall`
- `field_access` → chains field accesses → `Expr::FieldAccess`
- `ident_expr` → simple identifier → `Expr::Ident`

### Critical Parser Patterns Applied

**Issue 1**: `machine_item` is a wrapper rule in the grammar. Without unwrapping, the parser would fail to match actual items (state_decl, transition_decl, etc.).

**Solution**:
```rust
for item in body.into_inner() {
    // machine_item is a wrapper, get the actual item inside
    let actual_item = if item.as_rule() == Rule::machine_item {
        item.into_inner().next().unwrap()
    } else {
        item
    };
    // ... match on actual_item.as_rule()
}
```

**Issue 2**: Detecting async modifier requires a NAMED pest rule (not literal string).

**Solution**: `async_modifier` is a named rule in grammar.pest. Parser checks `Rule::async_modifier` in effect/handler inner pairs:
```rust
let is_async = pair.into_inner().any(|p| p.as_rule() == Rule::async_modifier);
```

This is a pest quirk: literal strings in grammar are anonymous and not accessible in Rule:: enum.

---

## 5. Rust Codegen (codegen.rs)

### What Each Gust Construct Generates

| Gust Construct | Rust Output |
|----------------|-------------|
| `type Foo { a: T, b: U }` | `pub struct Foo { pub a: T, pub b: U }` with `#[derive(Debug, Clone, Serialize, Deserialize)]` |
| `machine Bar { ... }` | State enum + machine struct + impl block + error type |
| `state X(a: T, b: U)` | Enum variant `X { a: T, b: U }` |
| `state Y` | Enum variant `Y` (unit variant if no fields) |
| `transition t: A -> B \| C` | Method `fn t(&mut self, ...) -> Result<(), Error>` with match on from-state |
| `effect e(p: T) -> R` | Trait method `fn e(&self, p: &T) -> R` |
| `on t(ctx: T) { ... }` | Handler body inside transition match arm |
| `async on t(ctx: T) { ... }` | `async fn t(&mut self, ...) -> Result<(), Error>` with `.await` on perform calls |
| `async effect e(p: T) -> R` | `async fn e(&self, p: &T) -> R` in trait |
| `goto State(args)` | `self.state = Enum::State { field: arg, ... }` |
| `perform effect(args)` | `effects.effect(args)` or `effects.effect(args).await` (if async) |
| `match expr { ... }` | Rust match with enum variants |
| `send channel(value)` | Channel send operation |
| `spawn child(args)` | Supervised child spawn |
| `timeout 5s` | `tokio::time::timeout(Duration::from_secs(5), ...)` wrapper |

### Key Codegen Patterns

#### 1. State Enum with Typed Variants

States with fields become enum variants with named fields:
```rust
pub enum OrderProcessorState {
    Pending { order: Order },
    Validated { order: Order, total: Money },
    Failed { reason: String },
}
```

States without fields become unit variants.

#### 2. Effect Trait Generation

If a machine declares effects, Gust generates a trait with `&self` methods:
```rust
pub trait OrderProcessorEffects {
    fn calculate_total(&self, order: &Order) -> Money;
    fn process_payment(&self, total: &Money) -> Receipt;
    fn create_shipment(&self, order: &Order) -> String;
}
```

**Design choice**: Effects use `&self` (not `&mut self`) because they're typically read-only queries or external I/O.

#### 3. Effects Context Threading

When emitting handler bodies, the codegen passes `&[EffectDecl]` through `expr_to_rust()` and `emit_statement()` so that `perform` expressions know whether to add `.await` (if the effect is async).

**Why**: Async effects must be awaited in async handlers. The effects context provides this information.

**Implementation**:
- `handler_uses_perform()` detects if handler uses any effects
- If true, adds `effects: &impl {Machine}Effects` parameter
- Pass effects slice through expression/statement emission
- When emitting `perform`, check if effect is async → add `.await` if true

#### 4. State Field Destructuring in Match Arms

When a handler runs, the from-state's fields are destructured in the match arm so handler code can access them:
```rust
match &self.state {
    OrderProcessorState::Pending { order } => {
        // Handler can use `order` directly
        let total = effects.calculate_total(order);
        // ...
    }
    _ => Err(OrderProcessorError::InvalidTransition { ... }),
}
```

**Why**: Handlers need access to the current state's data to make decisions.

#### 5. Goto Field Mapping

Arguments to `goto` are zipped with the target state's fields in declaration order:
```rust
// Gust: goto Validated(ctx.order, total);
// Rust:
self.state = OrderProcessorState::Validated {
    order: ctx.order,
    total: total,
};
```

**Implementation** (lines 395-415 in codegen.rs): Look up target state, zip args with fields, generate field initializers.

### Type Mapping (Rust)

| Gust Type | Rust Type |
|-----------|-----------|
| String | String |
| i64, i32, u64, u32, f64, f32, bool | Pass through |
| Vec\<T> | Vec\<T> |
| Option\<T> | Option\<T> |
| Result\<T, E> | Result\<T, E> |
| (T1, T2, ...) | (T1, T2, ...) (tuple) |
| Custom types | Pass through |
| Enums | Rust enums with data variants |
| Generic\<T> | Generic\<T> with trait bounds |

### Generated Code Structure

For each machine:
1. State enum with serde derives
2. Effect trait (if effects declared) with async methods
3. Machine struct with state field, channels, generic params
4. Error type for transitions
5. Impl block with:
   - Constructor (`new()`)
   - State accessor (`state()`)
   - Transition methods (one per transition, async if handler is async)
   - Channel accessors
6. Enum definitions for custom enums
7. Supervision runtime integration (if supervises children)

---

## 6. Go Codegen (codegen_go.rs)

### Handling Go's Lack of Sum Types

Go doesn't have Rust-style enums with data. Gust's solution:

1. **State constants via iota**:
```go
type OrderProcessorState int

const (
    OrderProcessorStatePending OrderProcessorState = iota
    OrderProcessorStateValidated
    OrderProcessorStateFailed
)
```

2. **Per-state data structs**:
```go
type OrderProcessorPendingData struct {
    Order Order `json:"order"`
}

type OrderProcessorValidatedData struct {
    Order Order `json:"order"`
    Total Money `json:"total"`
}
```

3. **Machine struct with optional state data pointers**:
```go
type OrderProcessor struct {
    State         OrderProcessorState           `json:"state"`
    PendingData   *OrderProcessorPendingData   `json:"pending_data,omitempty"`
    ValidatedData *OrderProcessorValidatedData `json:"validated_data,omitempty"`
    FailedData    *OrderProcessorFailedData    `json:"failed_data,omitempty"`
}
```

Only the active state's data pointer is non-nil. On state transitions, old data pointers are cleared.

### Effects as Go Interfaces

```go
type OrderProcessorEffects interface {
    CalculateTotal(order Order) Money
    ProcessPayment(total Money) Receipt
    CreateShipment(order Order) string
}
```

Methods use PascalCase (Go convention).

### PascalCase Conversion

Go uses PascalCase for exported identifiers. The codegen converts:
- Effect names: `calculate_total` → `CalculateTotal`
- Field names: `customer_id` → `CustomerId`
- State data struct fields: `order` → `Order`

**Implementation** (lines 616-626 in codegen_go.rs):
```rust
fn pascal_case(s: &str) -> String {
    s.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}
```

### Nil-Clearing on Goto

When transitioning to a new state, all state data pointers are set to nil, then the target state's data is initialized (lines 442-469 in codegen_go.rs):
```go
m.State = OrderProcessorStateValidated
m.PendingData = nil     // Clear all state data
m.ValidatedData = nil
m.FailedData = nil
m.ValidatedData = &OrderProcessorValidatedData{  // Set target state data
    Order: ctx.order,
    Total: total,
}
```

**Why**: Ensures only one state's data is active at a time.

### Async Support in Go

Go doesn't have Rust-style async/await. Gust's Go codegen uses `context.Context` as the first parameter for async functions:

```go
func (m *OrderProcessor) ValidateAsync(ctx context.Context, effects OrderProcessorEffects, ...) error {
    // context.Context allows cancellation and timeout propagation
}
```

Timeouts use `context.WithTimeout()`.

### Import Path Mapping

Gust use paths with `::` separators get mapped to Go imports with `/`:
```
// Gust: use std::time::Duration
// Go:   import "std/time"
```

### Smart Effects Param Injection (Go)

Same as Rust: `handler_uses_perform()` detects if handler uses effects, only then adds `effects OrderProcessorEffects` parameter.

### If-Condition Parens Stripping

Go if-statements don't use parentheses around conditions. The codegen strips outer parens:
```rust
// Gust: if (total.cents > 0) { ... }
// Go:   if total.cents > 0 { ... }
```

**Implementation** (lines 401-408 in codegen_go.rs):
```rust
let cond = self.expr_to_go(condition);
let cond = if cond.starts_with('(') && cond.ends_with(')') {
    &cond[1..cond.len()-1]
} else {
    &cond
};
self.line(&format!("if {cond} {{"));
```

### Type Mapping (Go)

| Gust Type | Go Type |
|-----------|---------|
| String | string |
| i64, i32, u64, u32 | int64, int32, uint64, uint32 |
| f64, f32 | float64, float32 |
| bool | bool |
| Vec\<T> | []T |
| Option\<T> | *T (nullable pointer) |
| Result\<T, E> | T + error return (Go convention) |
| (T1, T2, ...) | Anonymous struct (Go doesn't have tuples) |
| Enums | String-backed constants + switch for match |
| Generic\<T> | Go 1.18+ generics [T any] (ignores trait bounds) |

### JSON Marshaling Helpers

Generated Go code includes JSON helpers:
```go
func (m *OrderProcessor) ToJSON() ([]byte, error) {
    return json.MarshalIndent(m, "", "  ")
}

func OrderProcessorFromJSON(data []byte) (*OrderProcessor, error) {
    var m OrderProcessor
    if err := json.Unmarshal(data, &m); err != nil {
        return nil, err
    }
    return &m, nil
}
```

---

## 7. CLI (gust-cli/src/main.rs)

### Commands

**`gust build <file.gu>`**:
- Parse `.gu` file
- Generate code to `.g.rs`, `.g.go`, `.g.wasm.rs`, `.g.nostd.rs`, or `.g.ffi.rs` + `.g.h` alongside source (or to `-o` directory)
- Flags:
  - `--target rust|go|wasm|nostd|ffi` (default: rust)
  - `--package <name>` (for Go output, default: derived from filename)
  - `-o <dir>` (output directory, default: alongside source)
  - `--compile` (Rust only: run `cargo build` after generating)

**`gust watch <file.gu>`**:
- Watch `.gu` file and re-generate on changes
- 100ms debounce to avoid rapid re-generation
- Handles file deletions gracefully

**`gust parse <file.gu>`**:
- Parse `.gu` file and print AST debug output
- Used for debugging the parser

**`gust init <name>`**:
- Create new Gust project scaffold
- Sets up directory structure, Cargo.toml, example .gu file

**`gust check <file.gu>`**:
- Validate `.gu` file without generating code
- Checks state validity, transition validity, unreachable states, effect usage, goto arity

**`gust fmt <file.gu>`**:
- Format `.gu` file with 4-space indentation
- NOTE: Does not preserve comments (pest discards them)

**`gust diagram <file.gu>`**:
- Generate state machine diagram (placeholder - future Mermaid/Graphviz output)

### Output Convention

Generated files use `.g.{ext}` extension (inspired by C# source generators):

```
src/
  order_processor.gu           # Gust source (you write this)
  order_processor.g.rs         # Generated Rust (don't edit)
  order_processor.g.go         # Generated Go (don't edit)
  order_processor.g.wasm.rs    # Generated WASM Rust (don't edit)
  order_processor.g.nostd.rs   # Generated no_std Rust (don't edit)
  order_processor.g.ffi.rs     # Generated FFI Rust (don't edit)
  order_processor.g.h          # Generated C header (don't edit)
  effects.rs                   # Your effect implementations (you write this)
```

**Why this convention?**:
- Clearly marks generated code (don't edit)
- Lives alongside source for easy inspection
- Familiar to enterprise developers (C# background)
- Git can ignore `.g.*` files if desired
- Multiple targets can coexist

### Default Behavior

If no `-o` flag is provided, output is placed alongside the `.gu` source file:
```bash
gust build examples/order_processor.gu
# → examples/order_processor.g.rs

gust build examples/order_processor.gu --target go --package orders
# → examples/order_processor.g.go
```

### Example Usage

```bash
# Parse a .gu file (debug AST output)
gust parse examples/order_processor.gu

# Compile to Rust (default) — outputs .g.rs alongside the .gu file
gust build examples/order_processor.gu

# Compile to Go — outputs .g.go alongside the .gu file
gust build examples/order_processor.gu --target go --package orders

# Compile to a specific output directory
gust build examples/order_processor.gu -o src/generated/

# Compile and run cargo build
gust build examples/order_processor.gu --compile
```

---

## 8. Runtime (gust-runtime/src/lib.rs)

### Machine Trait

```rust
pub trait Machine: Serialize + for<'de> Deserialize<'de> {
    type State: Debug + Clone + Serialize + for<'de> Deserialize<'de>;

    fn current_state(&self) -> &Self::State;
    fn to_json(&self) -> Result<String, serde_json::Error>;
    fn from_json(json: &str) -> Result<Self, serde_json::Error> where Self: Sized;
}
```

**Purpose**: Common interface for all generated state machines. Provides JSON serialization and state inspection.

### Supervisor Trait & Runtime

```rust
pub trait Supervisor {
    type Error: Debug;

    fn on_child_failure(&mut self, child_id: &str, error: &Self::Error) -> SupervisorAction;
}

pub enum SupervisorAction {
    Restart,   // Restart the child machine from its initial state
    Escalate,  // Stop the child and propagate the error up
    Ignore,    // Ignore the failure and continue
}

pub struct ChildHandle {
    pub id: String,
    pub handle: JoinHandle<Result<(), Error>>,
}

pub struct SupervisorRuntime {
    pub strategy: RestartStrategy,
    pub children: Vec<ChildHandle>,
}

pub enum RestartStrategy {
    OneForOne,    // Restart only the failed child
    OneForAll,    // Restart all children if one fails
    RestForOne,   // Restart failed child and all started after it
}
```

**Purpose**: Structured concurrency support (Phase 3). SupervisorRuntime manages child processes with restart strategies. Generated code integrates when `supervises` annotation is used.

### Envelope

```rust
pub struct Envelope<T: Serialize> {
    pub source: String,
    pub target: String,
    pub payload: T,
    pub correlation_id: Option<String>,
}
```

**Purpose**: Message wrapper for cross-boundary communication. For future inter-machine messaging (Phase 3).

### Dependencies

- `serde` / `serde_json` → serialization
- `thiserror` → error types
- `tokio` → for future async support

---

## 9. What's Complete (All Phases)

### Phase 0: POC (Complete - commit 1935c7a)

- [x] PEG grammar for machine declarations (pest-based)
- [x] Full AST: machines, states, transitions, effects, handlers, expressions
- [x] Parser with `perform` as both statement and expression
- [x] Rust code generation (structs, enums, transitions, effects, handlers)
- [x] Go code generation (iota constants, data structs, interfaces, JSON)
- [x] CLI with `parse` and `build` commands
- [x] Runtime library with Machine/Supervisor traits

### Phase 1: Make It Real (Complete - commit 35e41c5)

- [x] Cargo `build.rs` integration (gust-build crate)
  - [x] Target enum (Rust/Go/Wasm/NoStd/Ffi)
  - [x] Incremental build support
- [x] `gust watch` command with 100ms debounce
- [x] Import resolution: `use` declarations resolve to Rust modules
- [x] Async support: `async` handlers and effects (tokio integration)
  - [x] Async detection via Rule::async_modifier (pest named rule)
  - [x] Effects context threading through expressions/statements
  - [x] `.await` on async perform calls
- [x] Type system improvements:
  - [x] Enums with data variants (TypeDecl enum with Struct/Enum cases)
  - [x] Pattern matching (`match` statements with Pattern::Variant)
  - [x] Tuple types (anonymous structs in Go)
  - [x] Option/Result handling (codegen pass-through)

### Phase 2: Developer Experience (Complete - commit 5f7632b)

- [x] VS Code extension (editors/vscode/)
  - [x] Syntax highlighting (TextMate grammar)
  - [x] File icons
  - [x] Snippets
  - [x] Language configuration (brackets, comments)
- [x] Language Server (gust-lsp crate)
  - [x] tower-lsp integration
  - [x] Diagnostics (parser errors, validation warnings)
  - [x] Go-to-definition
  - [x] Hover info
  - [x] Autocomplete
- [x] Error system (error.rs)
  - [x] GustError/GustWarning types
  - [x] SourceLocator for precise error locations
  - [x] Cargo-style colored output (colored crate)
- [x] Validator (validator.rs)
  - [x] State validity checking
  - [x] Transition target validation
  - [x] Unreachable state detection
  - [x] Dead transition detection
  - [x] Effect usage validation
  - [x] Goto arity checking (args match target state fields)
- [x] Tooling commands:
  - [x] `gust init` (project scaffold)
  - [x] `gust fmt` (formatter with 4-space indent, comments not preserved)
  - [x] `gust check` (validation without codegen)
  - [x] `gust diagram` (placeholder for state machine visualization)

### Phase 3: Structured Concurrency (Complete - Rust: 5f2ef6a, Go: 8a3fd0b)

- [x] Inter-machine channels (channel_decl in grammar/AST)
  - [x] Typed message passing
  - [x] Capacity specification
  - [x] Mode (broadcast/mpsc)
  - [x] Rust: tokio broadcast/mpsc channels
  - [x] Go: Go channels with buffering
- [x] Supervision trees
  - [x] `supervises` keyword in machine annotations
  - [x] SupervisorRuntime with RestartStrategy (OneForOne/OneForAll/RestForOne)
  - [x] ChildHandle with JoinHandle tracking
  - [x] spawn_named(), join_next(), restart_scope() methods
- [x] Lifecycle management
  - [x] `spawn` statement for supervised children
  - [x] Graceful shutdown (join all children)
  - [x] Timeout handling (timeout_spec in grammar, Duration with units ms|s|m|h)
  - [x] Cancellation support (tokio cancellation tokens)
- [x] Cross-boundary serialization
  - [x] In-process: direct channel passing
  - [x] NOTE: Network transport layer deferred (Phase 3 scoped to local only to avoid premature abstraction)

### Phase 4: Ecosystem (Complete - commits ce944aa → 64efc41)

- [x] Standard library (gust-stdlib/ crate)
  - [x] request_response.gu (generic request/response pattern)
  - [x] circuit_breaker.gu (generic with half-open state)
  - [x] saga.gu (multi-step with per-step compensation)
  - [x] retry.gu (exponential backoff with jitter)
  - [x] rate_limiter.gu (token bucket pattern)
  - [x] health_check.gu (heartbeat pattern)
- [x] Documentation structure (docs/)
  - [x] Specs: SPEC-PHASE1.md (2,365 lines), SPEC-PHASE2.md (2,909 lines), SPEC-PHASE3.md (1,946 lines), SPEC-PHASE4.md (3,561 lines)
  - [x] Guide: getting-started.md, language-reference.md, cookbook.md (placeholders - Codex filling content)
  - [x] API: runtime.md (placeholder - Codex filling content)
- [x] Example projects (examples/basic/, intermediate/, advanced/)
  - [x] Directory scaffolds created (Codex filling content)
- [x] Additional compilation targets:
  - [x] WASM (codegen_wasm.rs): #[wasm_bindgen], future_to_promise, JsValue fallback
  - [x] no_std (codegen_nostd.rs): heapless types, extern alloc
  - [x] C FFI (codegen_ffi.rs): #[repr(C)], handle API, null-safety, .h header generation
- [x] Generics support
  - [x] Generic machines with type parameters
  - [x] Trait bounds on generics (Rust)
  - [x] Go generics [T any] (ignores trait bounds, Go doesn't have Rust-style trait bounds)

### Test Suite (All Passing)

**27 tests total, zero clippy warnings**:
- gust-lang unit tests: 1 (parser basics)
- gust-lang integration tests: 16
  - language_semantics: 3 tests (async, enums, imports)
  - diagnostics_validation: 5 tests (validation, error reporting, LSP)
  - rust_codegen_concurrency: 3 tests (channels, supervision, timeouts)
  - go_codegen_concurrency: 2 tests (Go channel codegen, Go context)
  - generics_support: 3 tests (generic machines, trait bounds, stdlib)
  - target_backends: 3 tests (wasm, nostd, ffi)
  - docs_snippets: 1 test (doc generation)
  - import_resolution: 2 tests (use path resolution)
- gust-build: 2 tests (incremental builds, target selection)
- gust-runtime: 1 test (supervision runtime)
- gust-stdlib: 1 test (all stdlib machines parse and validate)

### Example Programs

The examples demonstrate all implemented features:
- `order_processor.gu` (original POC example)
- `gust-stdlib/*.gu` (6 reusable state machine patterns)
- Scaffold projects in `examples/basic/`, `intermediate/`, `advanced/` (Codex filling content)

---

## 10. What's Next (All Phases Complete)

### Completed Work

All four phases (Phase 1-4) from the roadmap are complete. The foundation is production-ready:

- ✅ Cargo integration (build.rs)
- ✅ Async support (tokio)
- ✅ Type system (enums, match, generics, tuples)
- ✅ VS Code extension
- ✅ Language server (LSP)
- ✅ Validation (states, transitions, unreachable detection, goto arity)
- ✅ Error system (cargo-style colored output)
- ✅ Tooling (init, fmt, check, watch, diagram placeholder)
- ✅ Structured concurrency (channels, supervision, spawn, timeouts)
- ✅ Standard library (6 generic patterns)
- ✅ Multiple targets (Rust, Go, WASM, no_std, FFI)
- ✅ Test suite (27 tests, all passing, zero clippy warnings)

### Remaining Work (Content & Hardening)

**Documentation Content** (structure exists, Codex filling):
- [ ] Complete getting-started.md tutorial
- [ ] Complete language-reference.md with all syntax
- [ ] Complete cookbook.md with common patterns
- [ ] Complete runtime.md API documentation
- [ ] Example projects content (basic/, intermediate/, advanced/)

**Hardening & Validation**:
- [ ] Generated code compilation testing (currently syntax-only validation)
- [ ] Property-based testing (quickcheck/proptest for parser/codegen)
- [ ] Formatter: preserve comments (pest limitation - requires custom lexer)
- [ ] Network transport layer (Phase 3 scoped to local - network deferred to avoid premature abstraction)
- [ ] `gust diagram` implementation (Mermaid or Graphviz output for state machines)

**Dogfooding**:
- [ ] Use Gust in a real project (Brock's goal)
- [ ] Collect feedback from actual usage
- [ ] Iterate on ergonomics based on real pain points
- [ ] Performance profiling on real workloads

**Community** (when ready for public use):
- [ ] Package registry (crates.io for gust-lang, gust-runtime, etc.)
- [ ] GitHub repo public (currently private under Dieshen)
- [ ] Contribution guidelines
- [ ] GitHub templates (issues, PRs)
- [ ] Discord/forum for community

**Future Enhancements** (post-v0.1):
- [ ] cargo-gust subcommand plugin
- [ ] Hot reload for development
- [ ] Performance optimizations based on profiling
- [ ] Additional stdlib patterns based on usage
- [ ] Additional compilation targets (if needed)

---

## 11. Key Design Decisions & Rationale

### 1. Transpile-to-Rust/Go Instead of Custom Runtime

**Decision**: Generate Rust/Go source code rather than building a custom interpreter or VM.

**Rationale**:
- No runtime overhead or learning curve
- Generated code is debuggable, familiar code
- Zero-cost abstractions (in Rust)
- Integrates with existing tooling (cargo, go build, debuggers)
- Teams can inspect and understand generated code
- No "black box" — generated code is transparent

**Trade-off**: More complex codegen, but simpler user experience.

### 2. `.g.rs` / `.g.go` Convention

**Decision**: Use `.g.{ext}` for generated files (inspired by C# source generators).

**Rationale**:
- Clearly marks generated code (don't edit)
- Lives alongside source for easy inspection
- Familiar to enterprise developers (Brock's C# background)
- Git can ignore generated files if desired
- No separate build artifacts directory needed

**Trade-off**: Slightly non-standard naming, but clear intent.

### 3. Effects as Traits/Interfaces

**Decision**: Generate an effect trait/interface for each machine with effects.

**Rationale**:
- Forces explicit dependency injection (testable)
- Makes side effects visible in function signatures
- No hidden I/O or database calls
- Idiomatic in both Rust (traits) and Go (interfaces)
- Easy to mock for testing

**Trade-off**: More verbose than just calling functions directly, but much clearer.

### 4. `perform` as an Expression

**Decision**: `perform` can be used as both a statement and an expression.

**Rationale**:
- Effects often return values (e.g., calculate total, process payment)
- `let total = perform calculate_total(order)` is natural
- Avoids needing mutable variables for effect results
- More functional style

**Trade-off**: Slightly more complex parser, but cleaner handler code.

### 5. "Service Contract" Positioning for Multi-Target

**Decision**: Same `.gu` file can target Rust or Go.

**Rationale**:
- Polyglot teams can share state machine definitions
- Service boundaries are language-agnostic
- Rust for performance-critical services, Go for rapid prototyping
- State machine logic is the same regardless of language
- Reduces duplication between services

**Trade-off**: Codegen is more complex (two backends), but opens up cross-language use cases.

### 6. State Field Destructuring in Match Arms

**Decision**: Destructure from-state fields in match arms.

**Rationale**:
- Handlers need access to current state's data
- Explicit field access (no magic `ctx.state.field` paths)
- Type-checked by Rust/Go compiler
- Clear what data is available in each handler

**Trade-off**: Slightly verbose match arms, but very clear.

### 7. Smart Effect Parameter Injection

**Decision**: Only add `effects` parameter if handler uses `perform`.

**Rationale**:
- Handlers that don't call effects shouldn't require an effects parameter
- Cleaner API for simple transitions
- Less boilerplate in user code
- Still type-safe (compiler enforces presence when needed)

**Trade-off**: Requires AST traversal to detect perform usage, but worth it for API clarity.

### 8. Async Modifier as Named Pest Rule

**Decision**: `async_modifier` is a named rule in grammar.pest, not a literal string.

**Rationale**:
- Pest quirk: literal strings in grammar are anonymous (not accessible in Rule:: enum)
- Named rules appear in Rule:: enum, allowing detection in parser
- Parser can check `p.as_rule() == Rule::async_modifier` to set `is_async` flag
- String comparison would fail silently

**Trade-off**: Slightly more verbose grammar, but necessary for correct parsing.

### 9. Effects Context Threading

**Decision**: Pass `&[EffectDecl]` through expression/statement emission in codegen.

**Rationale**:
- `perform` expressions need to know if the effect is async (to add `.await`)
- Effects context provides this information during codegen
- Avoids global state or multiple AST passes
- Type-safe: effects slice is immutable

**Trade-off**: More parameters to codegen functions, but explicit and clear.

### 10. Phase 3 Scoped to Local Concurrency Only

**Decision**: Phase 3 implements local structured concurrency (channels, supervision) but defers network transport layer.

**Rationale**:
- Avoid premature abstraction (don't design network layer without usage data)
- Local concurrency is sufficient for most use cases
- Network concerns (serialization format, discovery, retries, backpressure) are complex
- Better to dogfood local concurrency first, then design network layer based on real needs

**Trade-off**: Network-distributed machines require manual serialization for now, but avoids overengineering.

### 11. Go Generics Ignore Trait Bounds

**Decision**: Go codegen emits `[T any]` for generics, ignoring Rust trait bounds.

**Rationale**:
- Go doesn't have Rust-style trait bounds
- Go interfaces are structural, not nominal
- Attempting to map Rust bounds to Go interfaces would be complex and fragile
- `[T any]` is pragmatic: user must ensure types satisfy runtime requirements

**Trade-off**: Less type safety in Go, but avoids impedance mismatch between type systems.

---

## 12. Known Issues / Technical Debt

### Resolved Issues (All Phases Complete)

The following items from the POC have been resolved:
- ✅ Transition target validation (Phase 2 validator)
- ✅ User-friendly error messages (Phase 2 error.rs with colored output)
- ✅ Source location tracking (Phase 2 SourceLocator)
- ✅ Async support (Phase 1 async handlers/effects)
- ✅ Pattern matching (Phase 1 match statements)
- ✅ Tuple types (Phase 1 tuple support)
- ✅ Watch mode (Phase 1 gust watch)
- ✅ Cargo integration (Phase 1 gust-build)
- ✅ Validation before codegen (Phase 2 gust check)
- ✅ Supervision logic (Phase 3 SupervisorRuntime)
- ✅ Inter-machine messaging (Phase 3 channels)
- ✅ Integration tests (27 tests across all phases)
- ✅ Documentation structure (Phase 4 docs/)

### Remaining Known Issues

**Formatter**:
- Comments are not preserved (pest discards them during parsing)
- Requires custom lexer to preserve comments (deferred - low priority)

**Testing**:
- No generated code compilation testing (syntax validation only, not rustc/go build)
- No property-based testing (quickcheck/proptest for parser/codegen fuzzing)
- No roundtrip serialization tests (Gust → Rust → JSON → Rust → verify)

**Phase 3 Scope**:
- Network transport layer deferred (Phase 3 scoped to local structured concurrency only)
- Cross-network serialization requires manual setup for now
- Service discovery, retries, backpressure not addressed yet

**Documentation**:
- Docs pages are placeholders (Codex filling out content)
- Example projects are scaffolds (Codex filling out examples)

**Future Enhancements** (not issues, but potential improvements):
- Effect traits could support `&mut self` for stateful effects
- Go codegen could optimize nil-clearing (currently verbose but correct)
- `gust diagram` is placeholder (Mermaid/Graphviz implementation pending)

---

## 13. Development Process & History

### How Phases Were Developed

The development workflow was: **Claude writes detailed specs → Codex implements → Claude reviews with parallel agents → Codex fixes → Claude approves**. Each phase went through 1-2 review rounds before approval.

**Spec-Driven Development**:
1. Claude (Sonnet 4.5) wrote comprehensive phase specs (2,000-3,500 lines each)
2. Specs included: grammar changes, AST changes, parser logic, codegen patterns, test cases, examples
3. Codex reviewed specs and asked clarifying questions
4. Claude refined specs based on questions

**Implementation**:
1. Codex implemented features per spec
2. Codex ran tests after each major change
3. Codex committed incremental progress with conventional commits

**Review**:
1. Claude launched parallel review agents (Explore + Plan agents, Sonnet model)
2. Agents scanned codebase for: spec compliance, edge cases, error handling, idiomatic code
3. Agents reported findings to Claude
4. Claude synthesized review and provided feedback to Codex

**Iteration**:
1. Codex fixed issues from review
2. Codex re-ran tests, updated docs
3. Claude verified fixes with second review (lighter-weight)

**Approval**:
1. Claude approved phase completion
2. Codex tagged commit (e.g., phase1-complete, phase2-complete)
3. Moved to next phase

### Key Commits

- **POC**: 1935c7a (initial transpiler working)
- **Phase 1**: 35e41c5 (async, build.rs, watch, type system)
- **Phase 2**: 5f7632b (LSP, validation, errors, tooling)
- **Phase 3 Rust**: 5f2ef6a (channels, supervision, timeouts in Rust)
- **Phase 3 Go**: 8a3fd0b (Go context, channel codegen)
- **Phase 4**: ce944aa → 64efc41 (stdlib, docs, targets, generics)

### Lessons Learned

1. **Pest quirk**: Named rules vs literal strings (async_modifier must be named)
2. **Effects context threading**: Pass effect list through codegen to detect async
3. **Go generics**: Ignore trait bounds (Go doesn't have Rust-style bounds)
4. **Phase 3 scope**: Local-only concurrency (defer network to avoid premature abstraction)
5. **Spec-driven**: Detailed specs up-front reduce iteration cycles significantly
6. **Parallel review**: Multiple agents catch more issues than serial review

---

## 14. User Preferences & Context

### About the User (Brock)

- 20+ year enterprise C# developer with Rust experience
- Appreciates the C# `.g.cs` convention (applied as `.g.rs` / `.g.go` in Gust)
- Wants to use Gust in his own projects first before broader adoption
- Values clear, obvious code over clever tricks
- Prefers structured thinking and systematic problem-solving
- Likes to ask questions and clarify requirements before implementing

### Development Preferences

- **Subagent delegation**: Main agent orchestrates, subagents execute
  - Haiku for grunt work (file reading, searches, running tests)
  - Sonnet for code generation (features, bug fixes, tests)
  - Opus for architecture (system design, complex refactoring)
  - Target distribution: ~50% Haiku, ~40% Sonnet, ~10% Opus
- **Parallel dispatch**: Independent tasks run in parallel
- **Teams for big tasks**: 3+ interdependent steps = create a team
- **Ask questions**: Never assume, always clarify

### Project Context

- Personal/passion project, not commercial (yet)
- GitHub repo is private under Dieshen account
- Inspired by Rust, Go, Erlang/OTP, and C# source generators
- Goal: Make stateful services easier to write and maintain
- Long-term vision: Structured concurrency with supervision trees

### Code Style

- Clear, obvious code (no clever tricks)
- Descriptive naming
- Comment the "why", not the "what"
- Explicit error handling
- Measure before optimizing
- Simplicity first

---

## Quick Reference

### Build Commands

```bash
# Parse a .gu file (debug AST output)
gust parse examples/order_processor.gu

# Validate without codegen
gust check examples/order_processor.gu

# Format a .gu file (4-space indent, comments not preserved)
gust fmt examples/order_processor.gu

# Watch and auto-regenerate on changes
gust watch examples/order_processor.gu

# Compile to Rust (default)
gust build examples/order_processor.gu

# Compile to Go
gust build examples/order_processor.gu --target go --package orders

# Compile to WASM
gust build examples/order_processor.gu --target wasm

# Compile to no_std Rust
gust build examples/order_processor.gu --target nostd

# Compile to C FFI (generates .g.ffi.rs and .g.h)
gust build examples/order_processor.gu --target ffi

# Compile to specific directory
gust build examples/order_processor.gu -o src/generated/

# Compile and run cargo build
gust build examples/order_processor.gu --compile

# Initialize new project
gust init my_project

# Generate state machine diagram (placeholder)
gust diagram examples/order_processor.gu
```

### File Locations (Absolute Paths)

**Core crates**:
- Project root: `D:\Projects\gust`
- Grammar: `D:\Projects\gust\gust-lang\src\grammar.pest`
- AST: `D:\Projects\gust\gust-lang\src\ast.rs`
- Parser: `D:\Projects\gust\gust-lang\src\parser.rs`
- Error system: `D:\Projects\gust\gust-lang\src\error.rs`
- Validator: `D:\Projects\gust\gust-lang\src\validator.rs`
- Formatter: `D:\Projects\gust\gust-lang\src\format.rs`
- Rust codegen: `D:\Projects\gust\gust-lang\src\codegen.rs`
- Go codegen: `D:\Projects\gust\gust-lang\src\codegen_go.rs`
- WASM codegen: `D:\Projects\gust\gust-lang\src\codegen_wasm.rs`
- no_std codegen: `D:\Projects\gust\gust-lang\src\codegen_nostd.rs`
- FFI codegen: `D:\Projects\gust\gust-lang\src\codegen_ffi.rs`
- Runtime: `D:\Projects\gust\gust-runtime\src\lib.rs`
- CLI: `D:\Projects\gust\gust-cli\src\main.rs`
- Build integration: `D:\Projects\gust\gust-build\src\lib.rs`
- LSP: `D:\Projects\gust\gust-lsp\src\main.rs`, `D:\Projects\gust\gust-lsp\src\handlers.rs`

**Standard library**:
- Stdlib: `D:\Projects\gust\gust-stdlib\*.gu` (6 machines)

**VS Code extension**:
- Extension: `D:\Projects\gust\editors\vscode\package.json`
- Grammar: `D:\Projects\gust\editors\vscode\syntaxes\gust.tmLanguage.json`
- Snippets: `D:\Projects\gust\editors\vscode\snippets\gust.json`

**Documentation**:
- Roadmap: `D:\Projects\gust\docs\ROADMAP.md`
- Architecture: `D:\Projects\gust\docs\ARCHITECTURE.md`
- Specs: `D:\Projects\gust\docs\specs\SPEC-PHASE*.md` (4 files)
- Guide: `D:\Projects\gust\docs\guide\*.md` (3 files, placeholders)
- API: `D:\Projects\gust\docs\api\runtime.md` (placeholder)

**Examples**:
- Original example: `D:\Projects\gust\examples\order_processor.gu`
- Project scaffolds: `D:\Projects\gust\examples\{basic,intermediate,advanced}\` (Codex filling)

### Language Keywords

- `machine` → Declare a state machine
- `state` → Declare a state with optional typed fields
- `transition` → Declare a valid state transition (from → targets)
- `effect` → Declare a tracked side effect with signature
- `async` → Mark effect or handler as async
- `on` → Handle a transition with logic
- `goto` → Transition to a new state with field values
- `perform` → Execute a tracked effect (usable as expression)
- `match` → Pattern match on enums
- `send` → Send to channel
- `spawn` → Spawn supervised child
- `timeout` → Set timeout duration (with ms|s|m|h unit)
- `type` → Declare a data type (struct)
- `enum` → Declare an enumeration with variants
- `channel` → Declare a communication channel
- `sends` → Annotation: channels this machine sends to
- `receives` → Annotation: channels this machine receives from
- `supervises` → Annotation: child machines this supervises
- `use` → Import a module

### Critical Code Patterns

**Parser**:
- Machine item unwrapping (parser.rs): Unwrap wrapper rules to get actual rule type
- Async detection (parser.rs): Check `Rule::async_modifier` (named rule, not literal string)

**Codegen**:
- Effects context threading (codegen.rs, codegen_go.rs): Pass `&[EffectDecl]` to know which effects are async
- Smart effect injection (codegen.rs, codegen_go.rs): Only add effects param if handler uses perform
- State destructuring (codegen.rs): Match arms destructure from-state fields
- Goto field mapping (codegen.rs, codegen_go.rs): Args zipped with target state fields in declaration order
- Async await (codegen.rs): Add `.await` to perform calls when effect is async

**Validation**:
- Goto arity checking (validator.rs): Ensure goto args match target state field count
- Unreachable state detection (validator.rs): Graph traversal from initial state
- Transition target validation (validator.rs): Ensure targets exist

**Error Reporting**:
- SourceLocator (error.rs): Precise error locations with line/column
- Colored output (error.rs): Cargo-style error messages with colored crate

---

## Final Notes

This document represents the complete state of the Gust project as of **v0.1.0 with all four phases complete** (POC + Phase 1-4). A fresh Claude session should read this to immediately understand:

1. **What Gust is and why it exists**: Type-safe state machine DSL targeting Rust/Go/WASM/no_std/FFI
2. **Complete architecture**: 6 crates (lang, runtime, cli, build, lsp, stdlib) + VS Code extension
3. **Parser, AST, and codegen**: Pest-based parser, AST with async/channels/generics/enums, 5 codegen backends
4. **What's implemented**: All phases complete, 27 tests passing, zero clippy warnings
5. **What's next**: Documentation content, example projects, hardening, dogfooding
6. **Key design decisions**: Transpile-to-native, effects as traits, async as named pest rule, effects context threading, local-only concurrency
7. **Development process**: Spec-driven with parallel agent reviews
8. **User preferences and project context**: Brock (Dieshen), enterprise C# background, aggressive subagent delegation

**Current Status**: Production-ready foundation. Ready for dogfooding in real projects. Remaining work is content (docs, examples) and hardening (compilation tests, property-based testing).

**Next Steps**: Use Gust in a real project, collect feedback, iterate on ergonomics based on actual pain points.
