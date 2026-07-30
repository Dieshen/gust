---
title: "Guides"
description: "Task-oriented guides for moving existing code into Gust, running machines under Tokio, debugging, performance, and safe code generation."
type: guide
---

# Guides

Each guide here starts from something you are trying to do, not from a feature that needs explaining. They assume you have written a machine before. If you have not, work through the [Tutorial](../tutorial/) first, and keep the [Language Reference](../reference/) open beside you.

## Pick the guide that matches your problem

**[Migrating from Rust](migration_rust.md)** — you have a hand-written state machine in Rust and want it expressed as a `.gu` contract. The guide is mostly about what does *not* move: Gust has no loops, no method calls and no struct literals, so migration means deciding what becomes a state, what becomes a transition, and what gets pushed down into an effect.

**[Tokio Integration](tokio_integration.md)** — you are running a machine inside an async service. Covers what `async effect` lowers to and why the `Send` bound is there, what a `timeout` transition actually guards, and where the channel support currently stops.

**[Workflow Runtime Integration](workflow_runtime.md)** — you are building the durable-execution engine, not the application. Covers the `gust_parse` MCP contract, `effect` versus `action` replay semantics, and checkpointing generated Go.

**[Debugging](debugging.md)** — the compiler said something you did not expect, or the generated code did something you did not expect. Covers reading a diagnostic, what each warning is protecting you from, and the four tools that show you a machine from a different angle: `gust check`, `gust diagram`, `gust parse`, and `--tracing`.

**[Performance](performance.md)** — what a transition costs at runtime, described as mechanism rather than as numbers. There are no benchmarks in the repository, so this guide tells you what the code generator emits and leaves the measuring to you.

**[Security](security.md)** — the trust boundaries. Why `gust generate` refuses to write outside the manifest directory, which parts of the toolchain are *not* confined, and what a machine does and does not validate when you rehydrate it from a checkpoint.

## What every guide assumes

Two habits run through all of them, because they are the ones that catch real defects:

- **`gust check` is necessary, not sufficient.** It validates the `.gu` source. It does not promise the generated code compiles, and the Rust and Go backends are not equivalent. Build the output for every backend you ship and run it through the real toolchain — `clippy -D warnings` for Rust, `go vet ./...` for Go.
- **Never edit a `.g.rs` or `.g.go`.** They are overwritten on the next build. If the generated code is wrong, the fix belongs in the `.gu`, in your effects implementation, or in the compiler.
