# Gust

**A type-safe state machine language that compiles to Rust and Go.**

Write your state machines once in `.gu` files. Gust generates idiomatic, production-ready code for your target language. No boilerplate. No invalid states. No hidden side effects.

## Why Gust?

Most production bugs aren't algorithm bugs — they're state management bugs, unhandled edge cases at service boundaries, and functions that secretly talk to the database. Gust makes those structurally impossible.

- **Describe the state machine in 30 lines, get 300+ lines of correct code out**
- **Change a state or transition, regenerate** — no hunting through match arms
- **Same `.gu` file targets Rust and Go** — your service contract is language-agnostic

## Core Concepts

- **Algebraic State Machines** — Define states and transitions declaratively. The compiler enforces that only valid transitions can occur.
- **Effect Tracking** — Side effects (IO, network, database) are declared as effects. You know at a glance what a function does. You implement the effects, Gust generates the wiring.
- **Auto Serialization** — Rust output derives `Serialize`/`Deserialize`. Go output gets `json` struct tags. Cross-service boundaries are type-checked.
- **Multi-Target** — Same `.gu` source compiles to idiomatic Rust or Go. Teams don't have to agree on a runtime to agree on state machine definitions.

## Quick Start

```bash
# Build the compiler
cargo build --release

# Parse a .gu file (debug AST output)
gust parse examples/order_processor.gu

# Compile to Rust (default) — outputs .g.rs alongside the .gu file
gust build examples/order_processor.gu

# Compile to Go — outputs .g.go alongside the .gu file
gust build examples/order_processor.gu --target go --package orders

# Compile to a specific output directory
gust build examples/order_processor.gu -o src/generated/
```

## File Convention

Generated files use the `.g.rs` / `.g.go` extension (inspired by C# source generators):

```
src/
  order_processor.gu       # Gust source (you write this)
  order_processor.g.rs     # Generated Rust (don't edit)
  order_processor.g.go     # Generated Go (don't edit)
  effects.rs               # Your effect implementations (you write this)
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

Gust generates:
- **Rust**: State enum, machine struct, transition methods with `match` exhaustiveness, effect trait, serde derives
- **Go**: State constants via `iota`, per-state data structs, transition methods with runtime validation, effects interface, json struct tags

## Architecture

```
source.gu → Lexer → Parser → AST → RustCodegen → .g.rs
                                   → GoCodegen   → .g.go
```

| Crate | Purpose |
|-------|---------|
| `gust-lang` | Parser (pest PEG grammar), AST, Rust and Go code generators |
| `gust-runtime` | Runtime traits and utilities imported by generated Rust code |
| `gust-cli` | The `gust` command-line tool |

## Language Keywords

| Keyword | Purpose |
|---------|---------|
| `machine` | Declare a state machine |
| `state` | Declare a state with optional typed fields |
| `transition` | Declare a valid state transition (from -> targets) |
| `effect` | Declare a tracked side effect with signature |
| `on` | Handle a transition with logic |
| `goto` | Transition to a new state with field values |
| `perform` | Execute a tracked effect (usable as expression) |
| `type` | Declare a data type (struct) |
| `use` | Import a module |

## Roadmap

See [docs/ROADMAP.md](docs/ROADMAP.md) for the full phased roadmap.

**Current (v0.2 - Phase 1 complete):**
- [x] PEG grammar, parser, AST
- [x] Rust codegen with full handler body emission
- [x] Go codegen with idiomatic Go patterns
- [x] Multi-target CLI (`--target rust|go`)
- [x] Effect trait/interface generation
- [x] `.g.rs` / `.g.go` output convention
- [x] Cargo `build.rs` integration via `gust-build`
- [x] `gust watch` live recompilation mode
- [x] Async handlers/effects with Rust `.await` codegen
- [x] Enum declarations, tuple types, and `match` statements

**Next (Phase 2):**
- [ ] VS Code extension (syntax highlighting)
- [ ] LSP server
- [ ] Improved diagnostics and validation warnings
- [ ] Tooling commands (`gust init`, `gust fmt`, `gust check`, `gust diagram`)

## License

MIT
