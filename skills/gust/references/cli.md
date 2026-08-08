# Gust CLI reference

The crate is `gust-cli`; the binary it installs is `gust`.

```bash
cargo install gust-cli --locked        # from crates.io
cargo install --path gust-cli --locked # from a checkout of the Gust repo
```

`--locked` uses the committed `Cargo.lock` rather than re-resolving dependencies, which is what makes the install reproducible. The tradeoff is that you also pin to those older dependency versions until Gust publishes a refreshed lock. If the install fails *without* `--locked`, that's a signal Gust needs a dependency bump rather than something to paper over.

## Subcommands

| Command | Purpose |
|---|---|
| `build <FILE>` | Compile one `.gu` to a target language |
| `generate` | Compile everything described by a `gust.toml` manifest |
| `check <FILE>` | Parse + validate one file, no output written |
| `watch [DIR]` | Recompile `.gu` files on change (defaults to `.`) |
| `fmt <FILE>` | Format in place |
| `parse <FILE>` | Print the AST — for debugging the compiler |
| `diagram <FILE>` | Emit a Mermaid state diagram |
| `schema <FILE>` | Emit JSON Schema for types and machine states |
| `init <NAME>` | Scaffold a Gust-enabled Rust project |
| `doctor` | Report environment status and flag stale or missing generated files |

## build

```bash
gust build machine.gu
gust build machine.gu -o src/generated
gust build machine.gu --target go --package orders
gust build machine.gu --compile
gust build machine.gu --tracing
```

| Flag | Meaning |
|---|---|
| `-o, --output <DIR>` | Output **directory**. The filename is always derived from the input stem — you cannot rename the output. |
| `-t, --target <T>` | `rust` (default), `go`, `wasm`, `nostd`, `ffi` |
| `-p, --package <NAME>` | Go package name. Required for `--target go`, ignored otherwise. |
| `--compile` | After writing, invoke the host toolchain to typecheck the result |
| `--tracing` | Emit tracing instrumentation behind `#[cfg(feature = "tracing")]` (Rust) |

Output naming, all placed in `-o` if given or beside the source otherwise:

| Target | File |
|---|---|
| `rust` | `<stem>.g.rs` |
| `go` | `<stem>.g.go` |
| `wasm` | `<stem>.g.wasm.rs` |
| `nostd` | `<stem>.g.nostd.rs` |
| `ffi` | `<stem>.g.ffi.rs` (plus a C header) |

## generate and gust.toml

For anything beyond a file or two, describe the build once in a manifest:

```toml
[package]                      # optional, metadata only
name = "orders"
version = "0.1.0"
id = "com.example.orders"

[source]
root = "src/machines"          # directory to scan — required
include = ["**/*.gu"]          # optional glob filter
exclude = ["**/scratch/**"]    # optional

[targets.rust]
output = "src/generated"       # required
module = "machines"            # optional
tracing = false                # optional

[targets.go]
output = "../go/orders"        # required
package = "orders"             # required

[targets.schema]
output = "schema"              # required
id = "https://example.com/schema"   # optional
```

```bash
gust generate                          # all targets in ./gust.toml
gust generate --config path/gust.toml
gust generate --target rust            # just one target
gust generate --check                  # verify committed output is current
gust generate --allow-outside          # permit outputs outside the safe roots
```

Two things to know:

- **The manifest only supports `rust`, `go`, and `schema`.** `wasm`, `nostd`, and `ffi` are `build`-only — reach for `gust build` if you need them.
- **Outputs are sandboxed by default.** A manifest may only write beneath the directory holding it or the directory you invoked from, so running `gust generate` inside an unfamiliar repository cannot scatter files across the filesystem. `--allow-outside` lifts that, which is occasionally legitimate (emitting Go into a sibling repo) but worth being deliberate about.

`gust generate --check` is the CI form: it exits non-zero when a committed `.g.rs` or `.g.go` is stale relative to its `.gu`, which catches the "edited the source, forgot to regenerate" class of bug.

## check

```bash
gust check machine.gu
```

Parses and validates a **single file**, writing nothing. Exits non-zero on errors. Run this first when authoring — it's the fastest signal, and it surfaces validator warnings that matter across backends (an unused `let` is a warning in Rust but a hard error in Go).

For whole-project freshness rather than one file's validity, use `gust doctor` or `gust generate --check`.

## diagram and schema

```bash
gust diagram machine.gu                    # Mermaid to stdout
gust diagram machine.gu -o docs/states.mmd
gust diagram machine.gu --machine Checkout # one machine only

gust schema machine.gu -o schema/
gust schema machine.gu --machine Checkout
```

Both accept `-o, --output` and `-m, --machine <NAME>`. The diagram output is Mermaid `stateDiagram` text, so it renders directly in Markdown that supports Mermaid.

## watch

```bash
gust watch                              # watch ./ , target rust
gust watch src/machines --target go --package orders
```

Takes a directory (default `.`), plus `-t, --target` and `-p, --package`. Useful while iterating on a machine; not a substitute for `build.rs` integration in a real build.

## Invoking from outside the Gust repo

`cargo run -p gust-cli` only works **inside** the Gust workspace — `-p` selects a workspace member, so it fails elsewhere with `package(s) gust-cli not found in workspace`. From another project, either use the installed binary:

```bash
gust build src/machines/order.gu -o src/generated
```

or point cargo at the other workspace explicitly, which is worth it when you're changing the compiler and consuming it at the same time:

```bash
cargo run --manifest-path D:/Dev/rust/gust/gust-cli/Cargo.toml -- build src/machines/order.gu -o src/generated
```

Relative paths in the arguments still resolve against your current directory, not the manifest's.
