# Gust

[![CI](https://github.com/Dieshen/gust/actions/workflows/ci.yml/badge.svg)](https://github.com/Dieshen/gust/actions/workflows/ci.yml)
[![Docs](https://github.com/Dieshen/gust/actions/workflows/docs.yml/badge.svg)](https://github.com/Dieshen/gust/actions/workflows/docs.yml)
[![Security](https://github.com/Dieshen/gust/actions/workflows/security.yml/badge.svg)](https://github.com/Dieshen/gust/actions/workflows/security.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**A type-safe state machine language that compiles to Rust and Go.**

Write your state machines once in `.gu` files. Gust generates idiomatic, production-ready code for your target language. No boilerplate. No invalid states. No hidden side effects.

## Why Gust?

Most production bugs aren't algorithm bugs — they're state management bugs, unhandled edge cases at service boundaries, and functions that secretly talk to the database. Gust makes those structurally impossible.

- **Describe the state machine in 30 lines, get 300+ lines of correct code out**
- **Change a state or transition, regenerate** — no hunting through match arms
- **Same `.gu` file targets Rust and Go** — your service contract is language-agnostic

## Core Concepts

| Concept                      | Description                                                                                               |
| ---------------------------- | --------------------------------------------------------------------------------------------------------- |
| **Algebraic State Machines** | Define states and transitions declaratively. The compiler enforces that only valid transitions can occur. |
| **Effect Tracking**          | Side effects (IO, network, database) are declared as effects. You know at a glance what a function does.  |
| **Auto Serialization**       | Rust output derives `Serialize`/`Deserialize`. Go output gets `json` struct tags.                         |
| **Multi-Target**             | Same `.gu` source compiles to idiomatic Rust or Go.                                                       |

## Quick Start

```bash
# Build the compiler
cargo build --release

# Compile to Rust (default) — outputs .g.rs alongside the .gu file
gust build examples/order_processor.gu

# Compile to Go
gust build examples/order_processor.gu --target go --package orders

# Watch for changes and rebuild
gust watch src/

# Format .gu files
gust fmt src/

# Validate without generating code
gust check src/machines.gu

# Generate Mermaid state diagram
gust diagram src/machines.gu
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

## File Convention

Generated files use the `.g.rs` / `.g.go` extension (inspired by C# source generators):

```
src/
  order_processor.gu       # Gust source (you write this)
  order_processor.g.rs     # Generated Rust (don't edit)
  order_processor.g.go     # Generated Go (don't edit)
  effects.rs               # Your effect implementations (you write this)
```

## Architecture

```
source.gu -> Parser -> AST -> Validator -> RustCodegen -> .g.rs
                                         -> GoCodegen  -> .g.go
```

| Crate                           | Purpose                                                                |
| ------------------------------- | ---------------------------------------------------------------------- |
| [`gust-lang`](gust-lang/)       | Parser (pest PEG grammar), AST, validator, Rust and Go code generators |
| [`gust-runtime`](gust-runtime/) | Runtime traits and utilities imported by generated Rust code           |
| [`gust-cli`](gust-cli/)         | The `gust` command-line tool                                           |
| [`gust-lsp`](gust-lsp/)         | Language Server Protocol implementation for editor support             |
| [`gust-mcp`](gust-mcp/)         | Model Context Protocol server for AI-assisted Gust development         |
| [`gust-build`](gust-build/)     | Cargo build script integration (`build.rs`)                            |
| [`gust-stdlib`](gust-stdlib/)   | Standard library of reusable `.gu` machines                            |

## Editor Support

### VS Code

The [Gust VS Code extension](editors/vscode/) provides:

- Syntax highlighting for `.gu` files
- Diagnostics (errors and warnings)
- Hover documentation
- Go-to-definition
- Format on save
- Custom file icon

The LSP intentionally does not advertise rename or find-references yet because
the supported symbol operations are still current-file scoped.

## Language Keywords

| Keyword      | Purpose                                              |
| ------------ | ---------------------------------------------------- |
| `machine`    | Declare a state machine                              |
| `state`      | Declare a state with optional typed fields           |
| `transition` | Declare a valid state transition (`from -> targets`) |
| `effect`     | Declare a tracked side effect with signature         |
| `action`     | Declare a non-idempotent externally visible operation |
| `on`         | Handle a transition with logic                       |
| `goto`       | Transition to a new state with field values          |
| `perform`    | Execute a tracked effect (usable as expression)      |
| `type`       | Declare a data type (struct)                         |
| `enum`       | Declare a sum type                                   |
| `use`        | Import a module                                      |
| `match`      | Pattern match on values                              |
| `if`/`else`  | Conditional logic                                    |
| `let`        | Variable binding                                     |
| `async`      | Mark handlers and effects as asynchronous            |

## Release Status

**v0.2.0** — current public release.

- [x] PEG grammar, parser, AST
- [x] Rust and Go code generation
- [x] Multi-target CLI (`parse`, `build`, `watch`, `init`, `fmt`, `check`, `diagram`)
- [x] `gust-build` Cargo integration
- [x] Diagnostics and validation with suggestions
- [x] Async handlers/effects, enums, tuples, `match`
- [x] `action` keyword and handler-safety diagnostics for replay-aware runtimes
- [x] `EngineFailure` in `gust-stdlib`
- [x] JSON Schema code generation and `gust doctor`
- [x] Channels, supervision, lifecycle timeouts
- [x] Additional targets (`wasm`, `nostd`, `ffi`)
- [x] LSP with hover, diagnostics, go-to-definition, formatting
- [x] VS Code extension with syntax highlighting and file icon
- [x] MCP server for AI-assisted development
- [x] Standard library (`gust-stdlib`)
- [x] Documentation book (mdBook)

See [ROADMAP.md](ROADMAP.md) for what's next.

## Known Limitations

- Inter-machine communication is currently local in-process channels only. Network transport is intentionally deferred.
- Cross-file `use` declarations resolve types but cross-file go-to-definition in the LSP is not yet implemented.
- LSP rename and find-references are not advertised until symbol resolution becomes scope-aware enough to avoid unsafe textual edits.
- Context field (`ctx.field`) error locations point to the handler declaration rather than the exact field access expression.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, required validation commands, and PR expectations.

## License

[MIT](LICENSE)
