# Gust Compiler Architecture

## Overview

The Gust compiler is a transpiler that converts `.gu` source files into idiomatic Rust. It runs as a pipeline:

```
source.gu -> Lexer/Parser -> AST -> Codegen -> .g.rs
               (pest)      (ast.rs) (codegen.rs)
```

## Workspace Structure

```
gust/
  gust-lang/          # Core compiler library
    src/
      grammar.pest    # PEG grammar (pest)
      ast.rs          # AST node definitions
      parser.rs       # Pest pairs -> AST conversion
      codegen.rs      # AST -> Rust source emission
      lib.rs          # Public API
  gust-runtime/       # Thin runtime support library
    src/
      lib.rs          # Machine trait, Supervisor trait, Envelope
  gust-cli/           # CLI binary
    src/
      main.rs         # `gust build` and `gust parse` commands
  examples/
    order_processor.gu     # Example Gust program
    order_processor.g.rs   # Generated output
```

## Pipeline Stages

### 1. Grammar (grammar.pest)

PEG grammar parsed by [pest](https://pest.rs). Defines the syntax for:

- **Program structure**: `use`, `type`, `machine` declarations
- **Machine items**: `state`, `transition`, `effect`, `on` handler
- **Statements**: `let`, `return`, `if`, `goto`, `perform`, expression statements
- **Expressions**: Full precedence chain (or -> and -> cmp -> add -> mul -> unary -> primary)
- **Primary expressions**: literals, `perform` expressions, field access, function calls, identifiers, parenthesized expressions

Key design choice: `perform` is an **expression**, not just a statement. This allows `let x = perform effect(args)` — effects return values.

### 2. AST (ast.rs)

Strongly-typed AST nodes. Key types:

- `Program` -> top-level container (uses, types, machines)
- `MachineDecl` -> states, transitions, handlers, effects
- `Statement` -> Let, Return, If, Goto, Perform, Expr
- `Expr` -> IntLit, FloatLit, StringLit, BoolLit, Ident, FieldAccess, FnCall, BinOp, UnaryOp, Perform

### 3. Parser (parser.rs)

Converts pest `Pair` nodes into AST types. Each grammar rule has a corresponding `parse_*` function. Expression parsing follows the precedence chain: `parse_expr` -> `parse_or_expr` -> ... -> `parse_primary`.

### 4. Codegen (codegen.rs)

`RustCodegen` struct with indent-tracking string builder. Generates:

| Gust Construct | Rust Output |
|----------------|-------------|
| `type Foo { ... }` | `pub struct Foo { ... }` with serde derives |
| `machine Bar { ... }` | State enum + struct + impl + error type |
| `state X(a: T)` | Enum variant `X { a: T }` |
| `transition t: A -> B \| C` | Method `fn t(&mut self, ...) -> Result<(), Error>` with match on from-state |
| `effect e(p: T) -> R` | Trait method `fn e(&self, p: &T) -> R` |
| `on t(ctx: T) { ... }` | Handler body inside transition match arm |
| `goto State(args)` | `self.state = Enum::State { field: arg, ... }` |
| `perform effect(args)` | `effects.effect(args)` |

Key design choices:

- **State field destructuring**: Match arms destructure from-state fields so handler code can access them.
- **Effect traits**: Each machine with effects generates a `{Machine}Effects` trait. Transition methods accept `effects: &impl {Machine}Effects`.
- **Goto field mapping**: Arguments to `goto` are zipped with the target state's fields in declaration order.

## Runtime (gust-runtime)

Minimal runtime support that generated code imports:

- `Machine` trait: `current_state()`, `to_json()`, `from_json()`
- `Supervisor` trait: `on_child_failure()` -> `SupervisorAction`
- `Envelope<T>`: Cross-boundary message wrapper with correlation IDs

## CLI (gust-cli)

Two commands:

- `gust build <file.gu>` — Parse and generate `.g.rs` alongside the source (or to `-o` directory)
- `gust parse <file.gu>` — Print AST debug output

## Output Convention

Generated files use the `.g.rs` extension (inspired by C# source generators):

```
order_processor.gu      # Source (you write)
order_processor.g.rs    # Generated (don't edit)
```
