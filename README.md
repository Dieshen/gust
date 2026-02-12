# 🌬️ Gust

**A type-safe state machine language that compiles to Rust.**

Gust combines the ergonomics of Go's simplicity with Rust's type safety to create a language purpose-built for stateful services, data pipelines, and concurrent systems.

## Core Concepts

- **Algebraic State Machines** — Define states and transitions declaratively. The compiler enforces that only valid transitions can occur.
- **Effect Tracking** — Side effects (IO, network, database) are declared and tracked. You know at a glance what a function *does*.
- **Auto Serialization** — States and messages derive `Serialize`/`Deserialize` automatically. Cross-service boundaries are type-checked.
- **Structured Concurrency** — Supervision trees are first-class. Machines supervise other machines with defined failure strategies.

## Quick Start

```bash
# Build the compiler
cargo build --release

# Parse a .gu file (debug output)
cargo run -- parse examples/order_processor.gu

# Compile a .gu file to Rust
cargo run -- build examples/order_processor.gu

# Compile and also build the generated Rust
cargo run -- build examples/order_processor.gu --compile
```

## Syntax Overview

```gust
type Order {
    id: String,
    customer: String,
}

machine OrderProcessor {
    state Pending(order: Order)
    state Validated(order: Order, total: Money)
    state Failed(reason: String)

    transition validate: Pending -> Validated | Failed

    effect calculate_total(order: Order) -> Money

    on validate(ctx: ValidationCtx) {
        let total = perform calculate_total(ctx.order);
        if total.cents > 0 {
            goto Validated(ctx.order, total);
        } else {
            goto Failed("invalid total");
        }
    }
}
```

## Architecture

```
source.gu → Lexer → Parser → AST → Rust Codegen → .rs file → cargo build
```

| Crate | Purpose |
|-------|---------|
| `gust-lang` | Lexer, parser (pest), AST, Rust code generator |
| `gust-runtime` | Runtime traits and utilities imported by generated code |
| `gust-cli` | The `gust` command-line tool |

## Language Keywords

| Keyword | Purpose |
|---------|---------|
| `machine` | Declare a state machine |
| `state` | Declare a state with optional typed fields |
| `transition` | Declare a valid state transition |
| `effect` | Declare a tracked side effect |
| `on` | Handle a transition event |
| `goto` | Transition to a new state |
| `perform` | Execute a tracked effect |
| `type` | Declare a data type |
| `use` | Import a module |

## Roadmap

- [ ] Full handler body codegen (expressions, control flow)
- [ ] State field destructuring in transition handlers
- [ ] Tokio-based concurrency runtime
- [ ] Supervision trees
- [ ] Channel-based inter-machine messaging
- [ ] WASM compilation target
- [ ] LSP server for editor support
- [ ] `gust init` project scaffolding

## License

MIT
