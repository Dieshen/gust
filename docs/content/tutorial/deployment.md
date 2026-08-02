---
title: "Deployment"
description: "Choose between generating Gust output in build.rs and committing it, then wire the choice into CI."
type: tutorial
---

# Deployment

You have been running `gust build` by hand. That works while you are the only person touching the file and stops working the moment somebody edits the `.gu`, forgets to regenerate, and ships a binary built from yesterday's state graph.

There are two ways to stop that happening, and they suit different projects. This page sets up both so you can pick.

## Option A — generate during `cargo build`

The `gust-build` crate compiles your `.gu` files from a build script, so the output cannot go stale.

```toml "Cargo.toml"
[package]
name = "photo-pipeline"
version = "0.1.0"
edition = "2021"

[dependencies]
gust-runtime = "0.3"
serde = { version = "1", features = ["derive"] }
thiserror = "2"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "time"] }

[build-dependencies]
gust-build = "0.3"
```

```rust "build.rs"
fn main() {
    if let Err(err) = gust_build::compile_gust_files() {
        panic!("gust build failed: {err}");
    }
}
```

That is the whole setup. `compile_gust_files` finds every `.gu` under `src/`, compiles each to a `.g.rs` beside its source, skips any whose output is already current, and emits the `cargo:rerun-if-changed` lines that make Cargo's rebuild tracking correct.

```bash
cargo run
```

Your `include!("upload.g.rs")` is unchanged, and `src/upload.g.rs` now regenerates itself whenever the `.gu` moves.

For more control — a separate source directory, a different output directory, a non-Rust target — replace the body of `build.rs` with the builder instead:

```rust "build.rs"
use gust_build::{GustBuilder, Target};

fn main() {
    GustBuilder::new()
        .source_dir("gust_sources")
        .output_dir("src/generated")
        .target(Target::Rust)
        .compile()
        .unwrap();
}
```

::: callout warning "This is not a free lunch"
`gust-build` depends on the whole Gust compiler, so every consumer of your crate compiles `gust-lang` and its parser generator before they can compile you. On a cold build that is real time, and it lands on people who never asked for a state machine language. It also means the generated code never appears in a diff, so a change to the state graph reviews as a one-line `.gu` edit with invisible consequences.
:::

## Option B — commit the generated files

The alternative is to check `src/upload.g.rs` into git and never take a build dependency at all. Downstream builds stay fast, and a pull request that changes the state graph shows the resulting Rust next to the `.gu` change, which is exactly what a reviewer wants to see.

The cost is discipline. Nothing stops somebody editing the `.gu` and forgetting to regenerate — so make the build stop them.

Describe the project's codegen once, in a `gust.toml` beside `Cargo.toml`:

```toml "gust.toml"
[source]
root = "src"

[targets.rust]
output = "src"
```

Now one command regenerates everything:

```bash
gust generate
```

```text
Generated .../photo-pipeline/src/upload.g.rs
```

And one command asserts that the committed output is current:

```bash
gust generate --check
```

```text
Checked .../photo-pipeline/src/upload.g.rs
```

It exits non-zero when a `.g.rs` is stale:

```text
error: generated file '.../src/upload.g.rs' is stale; run `gust generate`
```

That is your CI gate.

```yaml ".github/workflows/ci.yml"
name: CI

on: [push, pull_request]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install the Gust compiler
        run: cargo install gust-cli --locked
      - name: Fail if generated code is stale
        run: gust generate --check
      - name: Validate the sources
        run: gust check src/upload.gu
      - run: cargo test
      - run: cargo clippy --all-targets -- -D warnings
```

Note the ordering: the staleness check runs before the tests, so a stale-output failure is reported as itself rather than as a confusing test failure.

## Choosing

| | Generate in `build.rs` | Commit the output |
| --- | --- | --- |
| Can the output go stale? | No | Only if CI does not check |
| Build dependency on the compiler | Yes — `gust-lang` and its parser generator | None |
| Codegen changes visible in review | No | Yes |
| Extra CI step | None | `gust generate --check` |
| Suits | Applications, internal services | Libraries, anything published, anything with a review culture |

If you are building an application and nobody downstream compiles your crate, take Option A and stop thinking about it. If you publish the crate, or if you want state-graph changes to be reviewable, take Option B and add the check.

Whichever you choose, `gust doctor` will tell you where you stand:

```bash
gust doctor
```

```text
Gust Doctor
===========

  [OK] Rust: rustc 1.96.1
  [OK] Cargo: cargo 1.96.1
  [OK] Go: go version go1.26.4 (optional)
  [OK] Gust: 0.3.0

.gu files: 1 found
  [OK] .../src/upload.gu -> upload.g.rs (up to date)

Validation:
  [OK] .../src/upload.gu: valid

Summary: no issues found. Environment looks good!
```

## Shipping the same machine to Go

The pipeline you built is portable. Point the compiler at a different backend and you get standalone Go with no Gust runtime dependency:

```bash
gust build src/upload.gu --target go --package photos -o ./go/photos
cd go/photos && go mod init photos && go vet ./...
```

`--package` is required for Go and ignored elsewhere. To make Go part of the manifest rather than a one-off command, add a target:

```toml "gust.toml"
[source]
root = "src"

[targets.rust]
output = "src"

[targets.go]
output = "go/photos"
package = "photos"
```

`gust generate` then emits both, and `gust generate --check` guards both.

::: callout warning "Validation does not promise the output compiles"
`gust check` validates Gust source. It says nothing about whether the Rust or Go it produces will build, and the two backends are not equivalent — some constructs that pass validation compile as Rust and fail as Go. Build and compile the output for every backend you actually ship, and use `clippy -D warnings` rather than plain `cargo check`, because that is what your consumers will run.
:::

## You are done

You built a photo upload pipeline that scans, stores, publishes, and rejects; tested every branch of it without a network; ran it under Tokio; put it behind a supervised queue; and wired its codegen into a build.

Where to go next:

- **[Reference](../reference/index.md)** — the precise description of every construct you used, and the ones you did not.
- **[Cookbook](../cookbook/index.md)** — worked recipes for retry, circuit breakers, sagas, and rate limiting, each as a machine you can lift.
- **[Guides](../guides/index.md)** — Tokio integration, debugging, performance, and migrating existing Rust state machines.
- **[Known Limitations](../appendix/known_limitations.md)** — the rough edges, in one place, before you hit them.
