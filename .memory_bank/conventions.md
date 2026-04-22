# Project Conventions

## Commit Style
Conventional Commits with optional scope:
- `feat(parser):`, `fix(lsp):`, `docs:`, `ci:`, `test:`, `refactor:`, `build:`

## Branch Naming
- Feature: `feat/<description>`
- Auto work: `auto/<type>/<description>`
- Dependabot: `dependabot/cargo/<dep-name>`

## Build Commands
```bash
cargo build --workspace                                              # Build
cargo test --workspace --all-targets --all-features                  # Test
cargo fmt --all -- --check                                           # Format check
cargo clippy --workspace --all-targets --all-features -- -D warnings # Lint

# Examples (excluded from workspace)
cargo test --manifest-path examples/event_processor/Cargo.toml
cargo test --manifest-path examples/microservice/Cargo.toml
cargo test --manifest-path examples/workflow_engine/Cargo.toml

# Go smoke test
gust build examples/order_processor.gu --target go --output _tmp --package smoke
cd _tmp && go mod init smoke && go vet ./...
```

## CI Workflow
File: `.github/workflows/ci.yml`
Steps (since 0.2): fmt check, clippy, workspace tests, rustdoc with `-D warnings -D rustdoc::broken_intra_doc_links`, example tests, Go smoke test, `cargo-llvm-cov` + Codecov, `cargo-audit`
PR automation: auto-labeling by crate, size labels

## Generated Files
- Extension: `.g.rs` (Rust), `.g.go` (Go), `.g.wasm.rs` (WASM), `.g.nostd.rs` (no_std), `.g.ffi.rs` (FFI), `.g.h` (C header)
- Never manually edit generated files

## Test Organization
- Unit tests: inline `#[cfg(test)]` modules
- Integration tests: `gust-lang/tests/*.rs` (16+ test files, 300+ tests)
- Example tests: separate `--manifest-path` invocations
- Stdlib tests: `gust-stdlib/tests/machine_tests.rs` + `integration_test.rs`
- MCP integration: `gust-mcp/tests/mcp_tools_test.rs`
- LSP integration: `gust-lsp/tests/lsp_features_test.rs`
- CLI integration: `gust-cli/tests/cli_integration.rs`
- Runtime: `gust-runtime/tests/runtime_tests.rs`

## PR Workflow
- Force-push to branches is blocked by the harness. Use `git commit` + `git push` (new commits) rather than `git push --force-with-lease` for PR-branch updates.
- Direct push to `master` is blocked. All changes land via PR + squash-merge.
- `--admin` merges are blocked. Don't bypass branch protection.
- Stale AI-generated branches: rebase onto current master if the new logic is ~mechanical; otherwise close with a note and regenerate against fresh master.
- When writing Bash heredocs in commit messages, use single-quoted `<<'EOF'` to avoid shell interpolation (otherwise `$()` and backticks get evaluated).
