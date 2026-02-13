# Gust Language - Session State Document

**Last Updated**: 2026-02-12
**Version**: v0.1.0 (POC Complete)
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

**Current Status**: v0.1 POC is complete. End-to-end transpilation works: `.gu` → Rust/Go compilable code.

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
│   ├── ROADMAP.md                   # Phased development plan
│   └── ARCHITECTURE.md              # Compiler architecture doc
├── examples/
│   ├── order_processor.gu           # Example Gust program
│   └── order_processor.g.rs         # Generated Rust output
├── gust-lang/                       # Core compiler library
│   ├── Cargo.toml                   # Dependencies: pest, pest_derive, thiserror
│   └── src/
│       ├── lib.rs                   # Public API (8 lines)
│       ├── grammar.pest             # PEG grammar (86 lines)
│       ├── ast.rs                   # AST node definitions (148 lines)
│       ├── parser.rs                # pest → AST conversion (434 lines)
│       ├── codegen.rs               # AST → Rust codegen (555 lines)
│       └── codegen_go.rs            # AST → Go codegen (669 lines)
├── gust-runtime/                    # Runtime support library
│   ├── Cargo.toml                   # Dependencies: serde, serde_json, thiserror, tokio
│   └── src/
│       └── lib.rs                   # Machine/Supervisor traits, Envelope (77 lines)
├── gust-cli/                        # CLI binary
│   ├── Cargo.toml                   # Dependencies: gust-lang, clap
│   └── src/
│       └── main.rs                  # gust build/parse commands (156 lines)
└── memory_bank/
    └── SESSION_STATE.md             # This document
```

### Crate Roles

| Crate | Purpose | Key Files |
|-------|---------|-----------|
| `gust-lang` | Core compiler: parser, AST, codegen | grammar.pest, ast.rs, parser.rs, codegen.rs, codegen_go.rs |
| `gust-runtime` | Runtime traits imported by generated code | lib.rs (Machine, Supervisor, Envelope) |
| `gust-cli` | CLI tool for building/parsing | main.rs (build, parse commands) |

---

## 3. Grammar & AST

### Grammar Rules Summary (grammar.pest)

**Top-level structure**:
- `program` → `(use_decl | type_decl | machine_decl)*`
- `use_decl` → `use` path segments
- `type_decl` → struct-like type with fields
- `machine_decl` → machine name + body

**Machine body items**:
- `state_decl` → state name + optional fields
- `transition_decl` → name : from_state -> target_states
- `effect_decl` → effect name + params + return type
- `on_handler` → handler for a transition with params and body

**Statements**:
- `let_stmt` → let bindings with optional type annotation
- `return_stmt` → return expression
- `if_stmt` → if/else with blocks
- `transition_stmt` → `goto` state with args
- `effect_stmt` → `perform` effect with args (statement form)
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

**Key Design Decision**: `perform` is both a statement and an expression. This allows `let x = perform effect(args)` — effects return values.

### AST Node Types (ast.rs)

**Top-level**:
- `Program { uses, types, machines }`
- `UsePath { segments }`
- `TypeDecl { name, fields }`
- `MachineDecl { name, states, transitions, handlers, effects }`

**Machine components**:
- `StateDecl { name, fields }` → state with optional typed fields
- `TransitionDecl { name, from, targets }` → valid state transition
- `EffectDecl { name, params, return_type }` → tracked side effect signature
- `OnHandler { transition_name, params, return_type, body }` → handler implementation

**Statements**:
- `Let { name, ty, value }` → variable binding
- `Return(Expr)` → return statement
- `If { condition, then_block, else_block }` → conditional
- `Goto { state, args }` → state transition
- `Perform { effect, args }` → effect invocation (statement form)
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

---

## 4. Parser (parser.rs)

### How It Works

The parser converts pest `Pair` nodes into AST types. Each grammar rule has a corresponding `parse_*` function.

**Entry point**: `parse_program(source: &str) -> Result<Program, String>`

**Key parsing functions**:
- `parse_machine_decl` → extracts states, transitions, effects, handlers
- `parse_state_decl` → state name + optional field list
- `parse_transition_decl` → transition name, from-state, target states
- `parse_effect_decl` → effect signature
- `parse_on_handler` → handler params, return type, body

**Expression parsing** (follows precedence chain):
- `parse_expr` → `parse_or_expr` → `parse_and_expr` → `parse_cmp_expr` → `parse_add_expr` → `parse_mul_expr` → `parse_unary_expr` → `parse_primary`

**Primary expression parsing**:
- Literals → direct conversion
- `perform_expr` → extracts effect name + args → `Expr::Perform`
- `fn_call` → extracts function name + args → `Expr::FnCall`
- `field_access` → chains field accesses → `Expr::FieldAccess`
- `ident_expr` → simple identifier → `Expr::Ident`

### Critical Parser Fix Applied

**Issue**: `machine_item` is a wrapper rule in the grammar. Without unwrapping, the parser would fail to match actual items (state_decl, transition_decl, etc.).

**Solution** (lines 98-102 in parser.rs):
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

This fix ensures we match on the actual rule type (state_decl, transition_decl, etc.) rather than the wrapper.

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
| `goto State(args)` | `self.state = Enum::State { field: arg, ... }` |
| `perform effect(args)` | `effects.effect(args)` (expression or statement) |

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

#### 3. Smart Effect Parameter Injection

The codegen uses `handler_uses_perform()` to detect if a handler body contains any `perform` calls. Only if true does it add the `effects: &impl {Machine}Effects` parameter to the transition method.

**Why**: Handlers that don't call effects shouldn't require an effects parameter (cleaner API).

**Implementation** (lines 525-554 in codegen.rs):
- Recursively walks the AST to detect `Expr::Perform` or `Statement::Perform`
- Returns true if any perform is found in the handler body
- Used at line 258-273 to conditionally add effects parameter

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
| Custom types | Pass through |

### Generated Code Structure

For each machine:
1. State enum with serde derives
2. Effect trait (if effects declared)
3. Machine struct with state field
4. Error type for transitions
5. Impl block with:
   - Constructor (`new()`)
   - State accessor (`state()`)
   - Transition methods (one per transition)

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

### Smart Effects Param Injection (Go)

Same as Rust: `handler_uses_perform()` detects if handler uses effects, only then adds `effects OrderProcessorEffects` parameter (lines 323-328 in codegen_go.rs).

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
- Generate code to `.g.rs` or `.g.go` alongside source (or to `-o` directory)
- Flags:
  - `--target rust|go` (default: rust)
  - `--package <name>` (for Go output, default: derived from filename)
  - `-o <dir>` (output directory, default: alongside source)
  - `--compile` (Rust only: run `cargo build` after generating)

**`gust parse <file.gu>`**:
- Parse `.gu` file and print AST debug output
- Used for debugging the parser

### Output Convention

Generated files use `.g.rs` / `.g.go` extension (inspired by C# source generators):

```
src/
  order_processor.gu       # Gust source (you write this)
  order_processor.g.rs     # Generated Rust (don't edit)
  order_processor.g.go     # Generated Go (don't edit)
  effects.rs               # Your effect implementations (you write this)
```

**Why this convention?**:
- Clearly marks generated code (don't edit)
- Lives alongside source for easy inspection
- Familiar to enterprise developers (C# background)
- Git can ignore `.g.rs` / `.g.go` files if desired

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

### Supervisor Trait

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
```

**Purpose**: For future structured concurrency support (Phase 3 in roadmap). Not yet used in generated code.

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

## 9. What's Complete (v0.1 POC)

### Fully Implemented Features

- [x] PEG grammar for machine declarations (pest-based)
- [x] Full AST: machines, states, transitions, effects, handlers, expressions
- [x] Parser with `perform` as both statement and expression
- [x] Rust code generation:
  - [x] Type declarations → Rust structs with serde derives
  - [x] State enums with typed variants
  - [x] Machine structs with constructors
  - [x] Transition methods with from-state enforcement
  - [x] State field destructuring in match arms
  - [x] Handler body codegen (let, if/else, goto, perform, return)
  - [x] Expression codegen (field access, function calls, binary/unary ops, literals)
  - [x] Effect trait generation with `&self` methods
  - [x] Smart effect parameter injection (only when handler uses perform)
- [x] Go code generation:
  - [x] State constants via iota
  - [x] Per-state data structs
  - [x] Effects as Go interfaces
  - [x] PascalCase conversion for Go conventions
  - [x] Nil-clearing on state transitions
  - [x] Smart effects param injection
  - [x] If-condition parens stripping
  - [x] JSON marshaling helpers
- [x] CLI with `parse` (AST debug) and `build` (codegen) commands
- [x] Multi-target support (`--target rust|go`)
- [x] `.g.rs` / `.g.go` output convention
- [x] Runtime library with Machine/Supervisor traits and Envelope messaging

### Example Program

The `examples/order_processor.gu` file demonstrates all working features:
- Two machines (OrderProcessor, OrderSupervisor)
- States with typed fields
- Transitions with multiple targets
- Effects with parameters and return types
- Handlers with if/else, perform expressions, goto statements
- Custom type declarations

---

## 10. What's Next (from ROADMAP.md)

### Phase 1: Make It Real

**Goal**: Use Gust in a real Rust project. Generated code compiles and runs.

- [ ] Cargo `build.rs` integration: auto-run gust compiler on `.gu` files during `cargo build`
- [ ] Or `cargo-gust` subcommand plugin
- [ ] `gust watch` — re-generate `.g.rs` on file save
- [ ] Import resolution: `use` declarations in `.gu` resolve to Rust modules
- [ ] Async support: `async` transition handlers and effects (tokio integration)
- [ ] Type system improvements: enums, Option/Result handling, pattern matching, tuples

### Phase 2: Developer Experience

**Goal**: Make writing Gust feel productive, not like fighting a tool.

- [ ] VS Code extension: syntax highlighting, file icons, snippets
- [ ] Language Server (LSP): diagnostics, go-to-definition, hover info, autocomplete
- [ ] Error messages: human-friendly parser errors with suggestions
- [ ] Transition validity checking before codegen
- [ ] Detect unreachable states and dead transitions
- [ ] Tooling: `gust init`, `gust fmt`, `gust check`, `gust diagram` (state machine visualization)

### Phase 3: Structured Concurrency

**Goal**: Multiple machines communicating with supervision. The full Erlang/OTP promise with static types.

- [ ] Inter-machine channels: typed message passing (tokio channels)
- [ ] Supervision trees: `supervises` keyword, strategies, automatic restart
- [ ] Lifecycle management: `spawn`, graceful shutdown, timeout handling, cancellation
- [ ] Cross-boundary serialization: in-process or across network boundaries

### Phase 4: Ecosystem

**Goal**: Other people can use Gust and contribute to it.

- [ ] Standard library: common patterns (request/response, saga, circuit breaker, retry policies)
- [ ] Documentation: language reference, tutorial, migration guide, cookbook
- [ ] Community: package registry, GitHub template, example projects
- [ ] Targets: WASM compilation, `no_std` support, C FFI generation

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

---

## 12. Known Issues / Technical Debt

### Parser

- No validation of transition targets (can reference non-existent states)
- No cycle detection in state graph
- Error messages are raw pest errors (not user-friendly)
- No source location tracking for better error reporting

### Codegen (Rust)

- No async support yet (all methods are sync)
- No Option/Result handling in handler bodies
- No pattern matching support
- No tuple types
- Effect traits always use `&self` (no `&mut self` support)

### Codegen (Go)

- No async/concurrency support (no goroutines/channels yet)
- Runtime state validation (not compile-time like Rust)
- Nil-clearing on goto is verbose (could be optimized)

### CLI

- No watch mode for auto-regeneration on file save
- No cargo integration (must run gust manually)
- No validation before codegen (catches errors late)

### Runtime

- Machine/Supervisor traits are defined but not implemented by generated code yet
- No actual supervision logic
- No inter-machine messaging

### Testing

- No integration tests for generated code
- No roundtrip serialization tests (Gust → Rust → JSON → Rust)
- No multi-target test suite (ensure Rust/Go output is semantically equivalent)

### Documentation

- No language reference
- No tutorial
- No migration guide
- No examples beyond order_processor.gu

---

## 13. User Preferences & Context

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

# Compile to Rust (default)
gust build examples/order_processor.gu

# Compile to Go
gust build examples/order_processor.gu --target go --package orders

# Compile to specific directory
gust build examples/order_processor.gu -o src/generated/

# Compile and run cargo build
gust build examples/order_processor.gu --compile
```

### File Locations (Absolute Paths)

- Project root: `D:\Projects\gust`
- Grammar: `D:\Projects\gust\gust-lang\src\grammar.pest`
- AST: `D:\Projects\gust\gust-lang\src\ast.rs`
- Parser: `D:\Projects\gust\gust-lang\src\parser.rs`
- Rust codegen: `D:\Projects\gust\gust-lang\src\codegen.rs`
- Go codegen: `D:\Projects\gust\gust-lang\src\codegen_go.rs`
- Runtime: `D:\Projects\gust\gust-runtime\src\lib.rs`
- CLI: `D:\Projects\gust\gust-cli\src\main.rs`
- Example: `D:\Projects\gust\examples\order_processor.gu`
- Roadmap: `D:\Projects\gust\docs\ROADMAP.md`
- Architecture: `D:\Projects\gust\docs\ARCHITECTURE.md`

### Language Keywords

- `machine` → Declare a state machine
- `state` → Declare a state with optional typed fields
- `transition` → Declare a valid state transition (from → targets)
- `effect` → Declare a tracked side effect with signature
- `on` → Handle a transition with logic
- `goto` → Transition to a new state with field values
- `perform` → Execute a tracked effect (usable as expression)
- `type` → Declare a data type (struct)
- `use` → Import a module

### Critical Code Sections

**Parser fix** (parser.rs:98-102): Machine item unwrapping
**Smart effect injection** (codegen.rs:258-273, codegen_go.rs:323-328): Only add effects param if handler uses perform
**State destructuring** (codegen.rs:286-304): Match arms destructure from-state fields
**Goto field mapping** (codegen.rs:395-415, codegen_go.rs:435-470): Args zipped with target state fields

---

## Final Notes

This document represents the complete state of the Gust project as of v0.1 POC completion. A fresh Claude session should read this to immediately understand:

1. What Gust is and why it exists
2. The complete architecture and compilation pipeline
3. How the parser, AST, and codegen work
4. What's implemented and what's next
5. Key design decisions and their rationale
6. User preferences and project context

The project is ready for Phase 1 work (cargo integration, async support, import resolution) to make Gust usable in real Rust projects.
