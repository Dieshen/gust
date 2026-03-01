# Contributing to Gust

Thanks for contributing. This document defines the minimum bar for changes to Gust.

## Setup

Prerequisites:
- Rust toolchain (stable)
- Go (stable)

Basic setup:

```bash
cargo build --workspace
```

## Development Workflow

1. Fork and create a focused branch from `main`.
2. Keep changes scoped to one logical concern.
3. Add or update tests with any behavior change.
4. Run validation locally before opening a PR.

## Required Local Validation

Run these before submitting:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo test -p gust-lang --test docs_snippets
```

If your change touches generated Go output or Go backend behavior, also run:

```bash
gust build examples/order_processor.gu --target go --output _tmp_go_smoke --package smoke
cd _tmp_go_smoke
go mod init smoke
go vet ./...
```

## Commit Style

Use Conventional Commits:

- `feat: ...`
- `fix: ...`
- `docs: ...`
- `refactor: ...`
- `test: ...`
- `ci: ...`
- `build: ...`

Examples:
- `fix(parser): report unknown transition target`
- `feat(go): add typed state payload constructors`

## Pull Requests

Each PR should:
- Link the related issue (or explain why one is not needed)
- Describe behavior changes and risk
- Include test evidence for affected crates
- Update docs when language, CLI, or behavior changes

## Reporting Bugs and Requesting Features

Use the GitHub issue forms:
- Bug report
- Feature request

When reporting bugs, include:
- Minimal `.gu` input
- Exact command used
- Full error output
- Rust/Go version and OS
