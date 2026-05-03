# FAQ

## Is Gust production-ready?

`v0.2.0` is intended as a stable release for core state-machine workflows:

- parse and validate `.gu` files
- generate Rust and Go code
- use CLI tooling (`build`, `watch`, `check`, `fmt`, `diagram`, `init`)
- use runtime/channel/supervision features shipped in this repository
- model replay-sensitive workflow operations with `effect` and `action`
- export JSON Schema for machine contracts

## Is Gust a replacement for Rust or Go?

No. Gust generates Rust or Go source. You keep full ownership of runtime integrations and effect implementations.

## Can I use `gust init` inside an existing Cargo workspace?

Yes. `gust init` now detects parent Cargo workspaces and automatically adds `[workspace]` to the generated `Cargo.toml` so nested projects build as standalone crates.

If you generated a project before this behavior was added and hit workspace errors:

- Add an empty `[workspace]` section to the generated project's `Cargo.toml`.
- Or move the project outside the parent workspace.

`gust init` project names must be Cargo-compatible (`[A-Za-z0-9_-]+`).

See [Known Limitations](known_limitations.md) for details.

## Does Gust support networked machine-to-machine transport?

Not yet. Inter-machine communication currently targets local in-process channels.

## Which targets are supported?

- `rust`
- `go`
- `wasm`
- `nostd`
- `ffi`

Use `gust build --target <target>` to generate output.
