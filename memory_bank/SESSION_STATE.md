# Gust Session State

**Last Updated**: 2026-05-12
**Version**: v0.2.1 release-prep snapshot
**Status**: Legacy continuity note

This `memory_bank` file is no longer the source of truth for durable project
knowledge. The global Obsidian vault is the durable knowledgebase; repo docs are
the source of truth for build, release, and architecture details.

Use these current files first:

- `README.md` - public project overview and release status
- `CHANGELOG.md` - release history
- `ROADMAP.md` - canonical roadmap and remaining work
- `docs/ARCHITECTURE.md` - compiler/workspace architecture
- `docs/src/` - mdBook source
- Obsidian `projects/gust/README.md` - durable cross-session project note

## Current Project Snapshot

Gust is a Rust 2024 workspace for a type-safe state machine language. The
workspace members are:

- `gust-lang` - parser, AST, validator, formatter, and code generators
- `gust-runtime` - runtime traits, envelopes, supervisors, and prelude
- `gust-cli` - command-line interface
- `gust-build` - Cargo build-script integration
- `gust-lsp` - Language Server Protocol implementation
- `gust-mcp` - Model Context Protocol server
- `gust-stdlib` - reusable `.gu` machines and `EngineFailure`

The `v0.2.1` release-prep work adds generated `gust:effect` /
`gust:action` annotations, normalizes release metadata, updates dependency
patch/minor versions, and moves CI coverage artifact upload to
`actions/upload-artifact@v7`.

## Current Validation

The release-prep tree has been validated with:

- `cargo check --workspace --all-targets`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-targets --all-features -q`
- `cargo doc --workspace --no-deps --all-features` with warnings and broken
  intra-doc links denied
- example project tests for `event_processor`, `microservice`, and
  `workflow_engine`
- Go codegen smoke test plus `go vet`
- `mdbook build docs` after installing `mdbook v0.5.2`
- `git diff --check`

## Current Release Scope

`v0.2.1` should remain a patch release:

- include PR #75 generated effect/action annotations
- include safe dependency and CI maintenance already prepared locally
- include release metadata and changelog updates
- do not include larger roadmap features
- keep PR #69 and PR #77 out of this release unless reviewed separately

## Remaining Near-Term Work

See `ROADMAP.md` for details. The short list is:

- finish remaining fine-grained source-span coverage
- add `gust test`
- add multi-file type resolution
- add cross-file LSP go-to-definition
- define VS Code/LSP bundling
- decide the future of `gust_new` in MCP versus external plugin workflow
