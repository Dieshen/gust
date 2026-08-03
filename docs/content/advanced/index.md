---
title: "Advanced"
description: "How the Gust compiler turns a .gu file into Rust, Go, and three specialist targets — and what that pipeline does and does not promise you."
type: reference
---

# Advanced

Everywhere else in these docs the compiler is a black box: you write a machine, you run `gust build`, and a `.g.rs` or `.g.go` appears. That is the right level to work at most of the time. This section is for when it is not.

Three things bring people here. You need to know what the generated code actually looks like, because you are integrating it into a codebase with opinions. You are sharing one set of contracts between a Rust service and a Go service, and you need generation to be reproducible and checkable in CI. Or you are eyeing a target Gust does not ship, and you want to know how far the compiler bends.

## What each page covers

**[Code Generation](codegen.md)** explains the pipeline — parser, AST, validator, backend — and what each stage is responsible for. It shows the real shape of generated Rust and Go, side by side, from the same source. It is also where the honest limits live: the backends are not equivalent, validation is not part of `gust build`, and generated Rust is not standalone.

**[Contract Packages](contract_packages.md)** is the multi-project workflow. One directory of `.gu` sources, one `gust.toml`, and a `gust generate` run that emits Rust, Go, and JSON Schema together. It covers the manifest schema, where a manifest is allowed to write, and how `--check` keeps committed output honest.

**[Custom Targets](custom_targets.md)** covers the three targets beyond Rust and Go — WebAssembly, `no_std`, and C FFI — and is blunt about what they emit, which is less than you might assume. It also answers the question the title invites: Gust has no plugin system, so adding a target of your own means either forking the compiler or building on `gust-lang` as a library.

## The one thing to take away first

The compiler has two independent jobs, and it will happily do the second without the first.

`gust check` parses and validates without emitting. `gust build`, `gust watch`, and `gust generate` validate and then emit — a semantic error stops them before anything is written. (In 0.3.0 only `check` validated, so generation from invalid source produced output that failed later in `rustc` or `go build`.)

Run `gust check` before `gust build`, and compile the result for every target you ship. The rest of this section assumes you do.
