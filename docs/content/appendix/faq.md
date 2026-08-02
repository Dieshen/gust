---
title: "FAQ"
description: "Short answers to the questions Gust raises most often."
type: reference
---

# FAQ

Short answers, with links to the pages that carry the detail.

## About the project

### Is Gust production-ready?

`v0.3.0` is a stable release for core state-machine workflows. What it covers:

- parsing and validating `.gu` files
- generating Rust and Go
- CLI tooling — `build`, `generate`, `watch`, `check`, `fmt`, `diagram`, `doctor`, `init`
- the runtime, channel, and supervision features shipped in this repository
- modelling replay-sensitive operations with `effect` and `action`
- consuming the generated `gust:effect` / `gust:action` annotations from a workflow runtime
- exporting JSON Schema for machine contracts

What it does not cover is listed on [Known Limitations](known_limitations.md). The honest summary: the Rust backend is the mature one, Go is close behind, and `wasm`, `nostd`, and `ffi` are narrower than their names suggest.

### Is Gust a replacement for Rust or Go?

No. Gust generates Rust or Go source that you own. You write every effect implementation and every runtime integration in the host language. Gust's job is the state machine — the enum, the legal transitions, and the effects trait — and nothing else.

### Which targets are supported?

`gust build --target <target>` accepts `rust` (default), `go`, `wasm`, `nostd`, and `ffi`. `go` also requires `--package <name>`.

JSON Schema is **not** a `build` target — `gust build --target schema` is rejected. Use the `gust schema` subcommand, or a `[targets.schema]` entry in a `gust.toml` manifest.

The five code targets are not equivalent. Read [Known Limitations](known_limitations.md#backends) before committing to anything other than `rust` or `go`.

### Does Gust support networked machine-to-machine transport?

Not yet. Channels and supervision are in-process. Cross-process and network transport are deliberately deferred rather than half-built.

## Writing `.gu`

### Why does my code fail to parse when it looks like valid Rust?

Because Gust is a much smaller language that happens to share Rust's surface. It has no loops, no method calls, no struct literals, no tuple values, no references, and no string escapes. Pattern-matching on Rust habits reliably produces code that does not parse.

The [Grammar](grammar.md) page lists every production and, more usefully, everything that is absent.

### How do I loop?

You don't — there is no loop construct. Two options:

- **Self-transition.** Declare `transition step: Running -> Running | Done`, carry an index in the state, and let the caller drive it. `Saga` in the standard library does exactly this.
- **Push it into an effect.** If the iteration is an implementation detail rather than part of the machine's observable behaviour, do it in Rust or Go behind a single effect.

### How do I call a method, index a collection, or build a struct?

Declare an effect. This is idiomatic rather than a workaround — `Saga` declares `len`, `get_step`, `push_step`, and `empty_steps` precisely because `.len()`, `[i]`, `.push()`, and `vec![]` do not exist.

Keep effects coarse enough to be worth implementing; each one becomes a trait method you have to write.

### What is the `ctx` parameter, and why is my argument missing from the generated method?

The `ctx` parameter gives a handler access to the fields of the state it is transitioning *from*. It is identified as **the first handler parameter whose type is not a declared type**, and it is then removed from the generated method signature — `GoCtx` in `on go(ctx: GoCtx)` is intentionally never declared.

Which means a typo in a parameter's type name silently turns that parameter into the accessor and drops it. `gust check` cannot catch this, because undeclared type names are legal by design. If an argument has vanished from generated code, check the spelling of the first parameter's type.

### `effect` or `action`?

Identical syntax, identical lowering; the keyword records intent for replay-aware runtimes.

- **`effect`** — idempotent and safe to replay: reading a row, computing a total.
- **`action`** — externally visible and *not* safe to replay: sending an email, posting a webhook.

Because a replay-aware runtime must checkpoint before an action, the validator enforces at most one action per code path, and requires it to be the last side-effectful step before the transition. Both are invoked with `perform`.

### Why does the validator warn about my unused `let`?

Because Rust merely warns about an unused local and Go rejects it outright (`declared and not used`), so the same `.gu` would build for one target and fail for the other. Reporting it at the source means you hear it once.

Both backends now lower an unread binding to a discard, so the output compiles either way and the effect still runs. The clearer way to say "run this, ignore the result" is the statement form:

```gust
machine Notifier {
    state Ready
    state Sent

    transition go: Ready -> Sent

    effect log(msg: String) -> ()

    on go() {
        perform log("sending");
        goto Sent;
    }
}
```

## Building and integrating

### Does `gust check` passing mean my code will compile?

No. `gust check` validates the Gust source; it makes no promise about any backend's output. Build the output for every target you actually ship, and run `clippy -D warnings` rather than plain `cargo check`, because that is what consumers do.

This is the lesson Gust's own test suite learned the hard way — three backends were emitting output no compiler had ever seen, and two of the three did not compile.

### Can I edit the generated `.g.rs` / `.g.go`?

No. They are overwritten on the next build. Commit them or generate them in `build.rs`, but treat them as output.

That includes not letting *tools* edit them. `cargo fmt` follows `mod` declarations, so wiring a `.g.rs` in with `#[path] mod` lets rustfmt silently reformat it — which then fails `gust generate --check` in CI. Prefer `include!`, which rustfmt does not follow.

### What do I need in `Cargo.toml` for generated Rust?

Generated Rust is not self-contained. It derives `Serialize` / `Deserialize`, derives `thiserror::Error` for the machine's error enum, and imports `gust_runtime::prelude::*`:

```toml
[dependencies]
gust-runtime = "0.3"
serde = { version = "1", features = ["derive"] }
thiserror = "2"
```

### Can I use `gust init` inside an existing Cargo workspace?

Yes. `gust init` detects a parent workspace and adds `[workspace]` to the generated `Cargo.toml` so the nested project builds standalone. Project names must be Cargo-compatible: `[A-Za-z0-9_-]+`.

For projects scaffolded before that behaviour existed, see [Known Limitations](known_limitations.md#gust-init-workspaces).

### How do I compile several `.gu` files to several targets at once?

Use a `gust.toml` manifest and `gust generate`. The manifest declares a `[source]` root with include and exclude globs, plus any of `[targets.rust]`, `[targets.go]`, and `[targets.schema]`. Paths resolve relative to the manifest file, not the shell's working directory.

Run `gust generate --check` in CI to assert that committed generated files are current. See [Contract Packages](../advanced/contract_packages.md).

### Why does `gust generate` refuse my output path?

Output paths are confined to the manifest directory or the current directory, checked after normalising `.` and `..`. Cloning a repository and running `gust generate` inside it is ordinary, and the `gust.toml` arrives with the repository — so a manifest must not be able to write wherever the invoking user can. Pass `--allow-outside` if you genuinely need to escape.

## Using the standard library

### How do I import `EngineFailure`?

```gust
use std::EngineFailure;

machine Job {
    state Running
    state Failed(failure: EngineFailure)

    transition abort: Running -> Failed

    effect classify() -> EngineFailure

    on abort() {
        goto Failed(perform classify());
    }
}
```

### Can I use the stdlib machines as-is?

Usually not without editing. They ship as embedded `.gu` **source**, not as compiled code, precisely so you can copy and adapt them — several bake constants into their handler bodies (a 60-second circuit-breaker timeout, a threshold of five) rather than exposing them as configuration.

They are also written in the bare source-state field style rather than the explicit `ctx:` form. That lowers correctly on both backends today, but the explicit form is clearer and is what the reference recommends. See [Stdlib API](stdlib_api.md).
