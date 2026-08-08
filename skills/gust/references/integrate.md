# Integrating generated code into a project

## Required dependencies (Rust)

Generated `.g.rs` is not self-contained. Every file emits `use serde::{Serialize, Deserialize}` and `use gust_runtime::prelude::*`, derives `Serialize`/`Deserialize` on types and state enums, and derives `thiserror::Error` on the machine's error enum. Missing any of these produces a wall of confusing errors in a file you didn't write:

```toml
[dependencies]
gust-runtime = "0.3"
serde = { version = "1", features = ["derive"] }
thiserror = "2"
```

The `derive` feature on serde is not optional. Keep `gust-runtime`'s minor version aligned with the `gust` binary that generated the code — the output assumes the trait shapes of its own version.

**All three are needed even though `gust-runtime`'s prelude re-exports them.** `gust_runtime::prelude` does `pub use serde::{Deserialize, Serialize}`, `pub use serde_json`, and `pub use thiserror`, which makes "the runtime already pulls those in, so one dependency is enough" a reasonable inference — and a wrong one. Generated code writes the *direct* paths `use serde::{Serialize, Deserialize};` and `#[derive(thiserror::Error)]`, and a re-export through another crate does not make `serde` or `thiserror` resolvable as top-level crate names. They must be direct dependencies.

## Bringing the file in

**Default to `include!`.** It splices the generated code into the current module, and — the part that matters — **rustfmt does not follow `include!`**, so `cargo fmt` leaves the generated file alone:

```rust
include!("processor.g.rs");
```

To get a namespaced module without exposing the file to rustfmt, wrap the `include!`:

```rust
pub mod order {
    include!("order.g.rs");
}
```

**Avoid `#[path]` mod for generated files:**

```rust
#[path = "generated/order.g.rs"]     // rustfmt WILL rewrite this file
mod order;
```

rustfmt follows `mod` declarations, so `cargo fmt` reformats the generated source in place — alphabetizing imports, collapsing struct variants. Verified: with `#[path] mod` the file is rewritten; with `include!` it is preserved.

That matters for two reasons. The rewritten file no longer matches what `gust build` emits, so **`gust generate --check` starts failing in CI** for a file nobody meaningfully changed; and every regeneration produces churn that reverses the formatting again, so `cargo fmt --check` and `gust generate --check` end up fighting each other.

`rustfmt.toml`'s `ignore` is **not** a fix — it's nightly-only and on stable it prints `unstable features are only available in nightly channel` and formats the file anyway. If you genuinely want `mod` semantics, generate into `OUT_DIR` from `build.rs` and `include!(concat!(env!("OUT_DIR"), "/order.g.rs"))`, which is outside the source tree rustfmt walks.

Either way, codegen does not modify your module tree. If the generated types are "not found", you almost certainly haven't wired the file in.

## Implementing the effects trait

A machine with effects generates `{Machine}Effects`. Methods take `&self` and borrow their arguments; the return type is whatever the `.gu` declared:

```gust
effect validate_event(event: Event) -> String
effect process_event(event: Event) -> ProcessedResult
```

```rust
struct ProductionEffects;

impl EventProcessorEffects for ProductionEffects {
    fn validate_event(&self, event: &Event) -> String {
        format!("validated:{}:p{}", event.source, event.priority)
    }

    fn process_event(&self, event: &Event) -> ProcessedResult {
        ProcessedResult { event_id: /* ... */, output: /* ... */ }
    }
}
```

Because effects are a trait, testing is straightforward: implement a second deterministic version and pass that instead. This is the main practical payoff of routing side effects through declarations rather than inlining them.

For `async effect`, the generated trait method is desugared to return-position `impl Future` rather than written as `async fn`, because `async fn` in a public trait doesn't promise the future is `Send` — and callers holding the machine across an await need that. Implementors can still write a plain `async fn`.

## Driving the machine

```rust
let mut machine = EventProcessor::new();

machine.receive(event)?;         // handler performs no effects — no effects arg
machine.validate(&effects)?;     // performs effects — takes them
machine.process(&effects)?;

match machine.state() {
    EventProcessorState::Completed { result } => { /* ... */ }
    EventProcessorState::Failed { reason } => { /* ... */ }
    _ => {}
}
```

Three things to note:

- **Transition methods return `Result`.** Calling one that isn't legal from the current state returns `Err`, it does not panic. This is the type-safety payoff — illegal sequences are runtime errors you can handle rather than undefined behavior.
- **Only handlers that `perform` something take an `effects` argument.** A handler that just rearranges state fields doesn't, which is easy to trip over when the signatures differ across transitions of the same machine.
- **State fields become named struct-variant fields**, named from the `.gu` declaration: `state Completed(result: ProcessedResult)` becomes `Completed { result }`.

## build.rs integration

`gust-build` compiles `.gu` during `cargo build`, so generated files never go stale:

```toml
[build-dependencies]
gust-build = "0.3"
```

```rust
// build.rs
fn main() {
    if let Err(err) = gust_build::compile_gust_files() {
        panic!("gust build failed: {err}");
    }
}
```

That discovers every `.gu` under `src/`, compiles each to `.g.rs` beside its source, skips files whose output is already current, and emits `cargo:rerun-if-changed` so Cargo's rebuild tracking is correct.

For more control:

```rust
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

`Target` variants: `Rust`, `Go { package_name }`, `Cffi`.

### Generate in build.rs, or commit the output?

Both are legitimate and the choice is about who needs to read the code.

- **`build.rs`** — output can't go stale, and `.g.rs` can be gitignored. Costs a build-dependency on the whole Gust compiler, which is a real compile-time hit for downstream consumers.
- **Commit the generated files** — no build dependency, and diffs are reviewable, which matters when you want codegen changes visible in PRs. Requires discipline: add `gust generate --check` to CI so a forgotten regeneration fails the build rather than silently shipping stale code.

## Go

```bash
gust build order.gu --target go --package orders -o ./orders
```

`--package` is required. The generated `.g.go` is standalone Go with no Gust runtime dependency — the Rust and Go backends are not symmetric here. Verify with the real toolchain, since `go vet` catches things string assertions don't:

```bash
cd orders && go mod init orders && go vet ./...
```

One asymmetry that bites: **Go rejects unused local variables outright** (`declared and not used`), where Rust merely warns. So a `.gu` with an unused `let` compiles as Rust and fails as Go. The validator warns about unused bindings against the `.gu` source precisely so you hear it once rather than as a backend-specific surprise — don't suppress it.

## Known rough edges

- **`gust init` scaffolds path dependencies.** The generated `Cargo.toml` contains `gust-runtime = { path = "../gust-runtime" }` and the same for `gust-build`. That only resolves inside the Gust repo itself. For a standalone project, replace them with version requirements.
- **`cargo run -p gust-cli` fails outside the Gust workspace.** `-p` selects a workspace member. Use the installed `gust` binary, or `cargo run --manifest-path <gust>/gust-cli/Cargo.toml -- ...`.
- **`-o` is a directory, not a file.** The output filename always derives from the input stem; you can't rename it at the CLI.
- **The `gust.toml` manifest supports only `rust`, `go`, and `schema`.** `ffi` is reachable only through `gust build --target ffi --unstable-ffi`.
- **`gust-build`'s own docs show `gust-build = "0.1"`** in the quick-start example. The current version is 0.3.
- **Generated Rust fails `clippy -D warnings` if the machine uses a `channel` or a `timeout`.** Those cause the prelude to emit `use tokio;`, which trips `clippy::single_component_path_imports`:

  ```
  error: this import is redundant
   --> src/slow.g.rs:4:1
    = note: `-D clippy::single-component-path-imports` implied by `-D warnings`
  ```

  This breaks a common CI setup on a file you're told never to edit. Until it's fixed upstream, allow the lint at the crate root:

  ```rust
  #![allow(clippy::single_component_path_imports)]
  ```

  or scope it to the module holding the generated code. All `tokio` references in the output are fully qualified, so the import is genuinely redundant — deleting it is the correct upstream fix.

## Verifying the integration

Generated code that parses but doesn't compile is the failure mode worth guarding against — it's happened to three of Gust's own backends. Run the real toolchain:

```bash
gust check src/machines/order.gu     # validate the source
cargo check                          # or `go vet ./...` for the Go target
```

`gust build --compile` does the Rust half of this in one step.
