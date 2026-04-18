# Contributing to Gust

Thanks for your interest in contributing to Gust. Whether you are fixing a bug, adding a feature, improving documentation, or reporting an issue, your help is welcome.

## Prerequisites

Before you begin, make sure you have the following installed:

- **Rust** (stable toolchain) -- install via [rustup](https://rustup.rs/)
- **Go** (stable) -- required for Go codegen testing
- **Git**

Verify your setup:

```bash
rustc --version
cargo --version
go version
```

## Getting Started

```bash
# Clone the repository
git clone https://github.com/Dieshen/gust.git
cd gust

# Build the entire workspace
cargo build --workspace

# Run all tests
cargo test --workspace --all-targets --all-features
```

## Project Structure

Gust is organized as a Cargo workspace with the following crates:

| Crate | Role |
|-------|------|
| `gust-lang` | Core compiler: PEG grammar (`grammar.pest`), parser, AST, validator, and code generators (Rust, Go, WASM, no_std, C FFI) |
| `gust-runtime` | Runtime traits (`Machine`, `Supervisor`, `Envelope`) imported by generated Rust code |
| `gust-cli` | The `gust` binary with subcommands: `build`, `watch`, `parse`, `init`, `fmt`, `check`, `diagram` |
| `gust-lsp` | Language Server Protocol implementation (tower-lsp) for editor support |
| `gust-mcp` | MCP server (JSON-RPC over stdin/stdout) for AI-assisted development |
| `gust-build` | Cargo build-script helper for `build.rs` integration |
| `gust-stdlib` | Standard library of reusable `.gu` machines (circuit breaker, retry, saga, rate limiter, etc.) |

The compiler pipeline flows as:

```
source.gu -> Parser (pest PEG) -> AST -> Validator -> Codegen -> .g.rs / .g.go
```

Key files in `gust-lang/src/`:

- `grammar.pest` -- PEG grammar defining Gust syntax
- `parser.rs` -- Pest pairs to AST conversion
- `ast.rs` -- Strongly-typed AST node definitions
- `validator.rs` -- Semantic validation with diagnostics
- `codegen.rs` -- Rust code generator
- `codegen_go.rs` -- Go code generator
- `codegen_wasm.rs`, `codegen_nostd.rs`, `codegen_ffi.rs` -- Additional target backends
- `format.rs` -- Gust source formatter

## Development Workflow

1. **Create a feature branch** from `master`:

   ```bash
   git checkout -b your-branch-name master
   ```

2. **Make your changes.** Keep them scoped to one logical concern.

3. **Add or update tests** for any behavior change.

4. **Run the full check suite** before submitting:

   ```bash
   # Format check
   cargo fmt --all -- --check

   # Lint (all warnings are errors in CI)
   cargo clippy --workspace --all-targets --all-features -- -D warnings

   # Workspace tests
   cargo test --workspace --all-targets --all-features

   # Example tests (excluded from workspace, must be run separately)
   cargo test --manifest-path examples/event_processor/Cargo.toml
   cargo test --manifest-path examples/microservice/Cargo.toml
   cargo test --manifest-path examples/workflow_engine/Cargo.toml
   ```

5. **Run the Go codegen smoke test** if your change touches code generation or Go output:

   ```bash
   cargo run -p gust-cli -- build examples/order_processor.gu \
       --target go --output _tmp_go_smoke --package smoke
   cd _tmp_go_smoke
   go mod init smoke
   go vet ./...
   cd ..
   rm -rf _tmp_go_smoke
   ```

6. **Commit your changes** following the commit conventions below.

7. **Open a pull request** against `master`.

## Commit Conventions

This project uses [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/). Every commit message should follow this format:

```
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

### Types

| Type | When to use |
|------|-------------|
| `feat` | A new feature |
| `fix` | A bug fix |
| `docs` | Documentation only |
| `refactor` | Code restructuring without behavior change |
| `test` | Adding or updating tests |
| `perf` | Performance improvement |
| `build` | Build system or dependency changes |
| `ci` | CI/CD configuration changes |
| `chore` | Maintenance tasks |
| `style` | Formatting, whitespace (no logic change) |

### Scopes

Use a scope to indicate which part of the codebase is affected:

`parser`, `validator`, `codegen`, `cli`, `lsp`, `mcp`, `runtime`, `build`, `stdlib`

### Examples

```
fix(parser): report unknown transition target
feat(codegen): add typed state payload constructors for Go
test(validator): cover duplicate state name detection
docs: update README with new CLI subcommands
refactor(lsp): extract hover logic into separate module
```

### Breaking Changes

Append `!` after the type/scope for breaking changes:

```
feat(codegen)!: change effect trait signature to accept context
```

Or use a `BREAKING CHANGE:` footer in the commit body.

## Pull Request Guidelines

When opening a pull request:

- **Link the related issue**, or explain why one is not needed.
- **Describe what changed** and why. Include any behavior changes and risk areas.
- **Include test evidence** for affected crates (new tests, updated tests, or an explanation of existing coverage).
- **Update documentation** when your change affects the language, CLI behavior, or generated output.
- **Use draft PRs** for work in progress. This signals that review is not yet needed.

CI runs format checking, clippy, workspace tests, example tests, and a Go codegen smoke test on every PR. All checks must pass before merging.

## Code Style

- **Run `cargo fmt`** before committing. CI enforces `cargo fmt --all -- --check`.
- **All clippy warnings are errors.** CI runs `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- **Follow existing patterns** in the crate you are modifying. Read the surrounding code before adding new patterns.
- **Naming**: use descriptive names. `process_text_nodes()` over `ptn()`.
- **Comments**: explain _why_, not _what_. The code shows what it does.

## Testing Guidelines

### Where to add tests

| Change area | Where to add tests |
|-------------|--------------------|
| Grammar or parser changes | `gust-lang/tests/language_semantics.rs` or `gust-lang/tests/parser_property_tests.rs` |
| Validator changes | `gust-lang/tests/diagnostics_validation.rs` |
| Rust codegen changes | `gust-lang/tests/generated_code_compilation.rs` or `gust-lang/tests/rust_codegen_concurrency.rs` |
| Go codegen changes | `gust-lang/tests/go_codegen_concurrency.rs` + the Go smoke test |
| Import/use resolution | `gust-lang/tests/import_resolution.rs` |
| Generics | `gust-lang/tests/generics_support.rs` |
| Additional backends (WASM, no_std, FFI) | `gust-lang/tests/target_backends.rs` |
| Documentation code examples | `gust-lang/tests/docs_snippets.rs` |
| CLI behavior | `gust-cli/` unit tests or integration tests |
| Runtime traits | `gust-runtime/` unit tests |
| LSP behavior | `gust-lsp/` tests |
| stdlib machines | `gust-stdlib/` tests |

### Running specific tests

```bash
# Run tests for a single crate
cargo test -p gust-lang

# Run a specific integration test file
cargo test -p gust-lang --test language_semantics

# Run a specific test by name
cargo test -p gust-lang -- test_name
```

### Integration test patterns

Integration tests in `gust-lang/tests/` typically follow this pattern:

1. Define a `.gu` source string inline.
2. Parse it through the compiler pipeline.
3. Assert on the AST, validator diagnostics, or generated output.

When adding a new test, look at existing tests in the same file for the expected structure.

## Generated Files

Files with the `.g.rs` and `.g.go` extensions are generated by the Gust compiler. **Never edit these files manually.** They will be overwritten on the next compilation.

The naming convention is inspired by C# source generators:

```
order_processor.gu       # Source (you write this)
order_processor.g.rs     # Generated Rust (don't edit)
order_processor.g.go     # Generated Go (don't edit)
```

## Reporting Issues

### Bug Reports

Use the [bug report template](https://github.com/Dieshen/gust/issues/new?template=bug_report.yml) on GitHub. Include:

- A minimal `.gu` input that reproduces the problem
- The exact command you ran
- Full error output (stderr and stdout)
- Your Rust version (`rustc --version`), Go version (`go version`), and operating system

### Feature Requests

Use the [feature request template](https://github.com/Dieshen/gust/issues/new?template=feature_request.yml) on GitHub. Describe the use case and the behavior you would like to see.

### Security Vulnerabilities

Do **not** open a public issue for security vulnerabilities. See [SECURITY.md](SECURITY.md) for the responsible disclosure process.

## License

Gust is licensed under the [MIT License](LICENSE). By contributing, you agree that your contributions will be licensed under the same terms.
