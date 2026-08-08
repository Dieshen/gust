---
name: gust-dev
description: Work on the Gust compiler itself — the Rust workspace at D:\Dev\rust\gust containing gust-lang, gust-cli, gust-runtime, gust-lsp, gust-mcp, gust-build, and gust-stdlib. Use this whenever changing grammar.pest, the parser, the validator, any codegen backend (Rust/Go/JSON Schema/C-FFI), the formatter, the LSP, or the MCP server; when adding a Gust language feature or validator diagnostic; when writing tests for the compiler; or when running cargo-mutants on it. Adding a language feature touches an ordered chain of a dozen places across the compiler, the LSP, the MCP server, and the editor grammar — skipping one silently breaks a backend or leaves tooling behind, so consult this skill before editing the compiler. For *writing* .gu files rather than changing the compiler, use the `gust` skill instead.
---

# Developing the Gust compiler

Gust is a Cargo workspace that compiles `.gu` state machine files to Rust and Go. This skill is about changing the compiler. If the task is authoring `.gu` source, the `gust` skill covers the language.

## Pipeline

```
source.gu → Parser (pest PEG) → AST → Validator → Codegen → .g.rs / .g.go
```

| Crate | Role |
|---|---|
| `gust-lang` | The compiler: grammar, parser, AST, validator, all codegen backends |
| `gust-runtime` | Thin traits (`Machine`, `Supervisor`, `Envelope`) that generated Rust imports |
| `gust-cli` | The `gust` binary — `build`, `generate`, `check`, `watch`, `fmt`, `parse`, `diagram`, `schema`, `init`, `doctor` |
| `gust-lsp` | Language server (tower-lsp) — diagnostics, hover, go-to-definition, formatting |
| `gust-mcp` | MCP server exposing compiler tools for AI-assisted development |
| `gust-build` | `build.rs` helper for compiling `.gu` during `cargo build` |
| `gust-stdlib` | Reusable `.gu` machines (circuit breaker, retry, saga, rate limiter…) |

Key files in `gust-lang/src`: `grammar.pest`, `ast.rs`, `parser.rs`, `validator.rs` (the largest), `codegen.rs` (Rust), `codegen_go.rs`, `codegen_ffi.rs`, `codegen_schema.rs`, `codegen_common.rs` (shared helpers + Mermaid), `format.rs`, `error.rs`.

## Adding a language feature: the chain

A new syntax form touches these in order. The failure mode is doing the first four and shipping — the code compiles, tests pass, and one backend silently emits nothing for the new form.

**Compiler:**

1. **`grammar.pest`** — add the rule and wire it into its parent (a rule nothing references is dead).
2. **`ast.rs`** — add the node. Carry a `Span` if it's a construct the validator will report on.
3. **`parser.rs`** — add a `parse_*` function; the convention is one per grammar rule, named after it.
4. **`validator.rs`** — add the semantic checks. What's syntactically expressible but semantically wrong?
5. **Every codegen backend** — `codegen.rs`, `codegen_go.rs`, and `codegen_schema.rs` if the form affects types or states; `codegen_ffi.rs` is behind `--unstable-ffi` and outside the 1.0 promise. Three shipped backends, and they are not symmetric.
6. **`format.rs`** — the formatter must round-trip the new form or `gust fmt` will silently destroy it.

**Tooling that lags silently if you skip it** — nothing fails, the feature just isn't supported by the editor experience:

7. **`gust-lsp`** — hover and diagnostics. Note the hover logic is **duplicated in both `lib.rs` and `main.rs`**; updating one and not the other is an easy miss.
8. **`gust-mcp`** — `gust_parse` emits the AST as JSON, so a new field needs adding there or downstream consumers (Corsac) can't see it.
9. **`editors/vscode/syntaxes/gust.tmLanguage.json`** — the keyword list is hardcoded (`\b(goto|perform|async|timeout)\b`), so a new keyword gets no highlighting.

**Then:**

10. **Docs** — `docs/src/` pages, and remember `docs_snippets.rs` compiles every ```` ```gust ```` block, so a snippet using the new form must parse.
11. **`CHANGELOG.md`**.
12. **Tests** — including a fixture in `codegen_backends.rs`, which is the only thing that proves the output actually compiles.

If a form genuinely doesn't apply to a backend, make that an explicit decision with a comment, not an omission. `ffi` already ignores `timeout`, so ignoring an analogous new annotation is defensible — say so in writing rather than leaving a silent hole.

## The central test invariant

**Asserting on generated strings does not tell you whether the output compiles.** This is the most important lesson the codebase has learned, and it's worth stating plainly: three backends — wasm, no_std, and ffi — had never once had their output fed to a compiler, and two of the three did not compile at all when someone finally tried. `wasm` and `nostd` were deleted in 1.0 rather than frozen into the stability promise; `ffi` is gated behind `--unstable-ffi`.

`gust-lang/tests/codegen_backends.rs` fixes this structurally. It holds a table of `.gu` fixtures and compiles each one's output with each backend's real toolchain — `rustc` for the Rust-family targets, `go vet` for Go. Adding a fixture exercises every backend at once, so coverage can't drift backend-by-backend the way it did before.

So: when you add a language feature, **add a fixture there**. Don't just assert the emitted string contains the right substring. `references/testing.md` has the fixture layout and the rest of the test map.

## Where diagnostics come from

The validator produces `GustError` and `GustWarning`, both carrying `file`, `line`, `col`, `message`, plus optional `note` and `help`. They render in rustc's visual style with a source-annotated caret block.

The convention is worth respecting because it's what makes the diagnostics feel good to use:

- **`message`** — what is wrong, stated concisely.
- **`note`** — why it's wrong, or what rule was violated.
- **`help`** — what to do about it. Did-you-mean suggestions use `strsim` for fuzzy matching against declared names.

Errors block compilation; warnings don't. Choosing between them has a real consequence: an unused `let` is only a warning in Rust but a hard error in Go, so the validator warns against the `.gu` so the author hears it once at the source rather than as a backend-specific surprise.

Type inference is deliberately conservative. `TypeContext` infers expression types for goto-argument, let-annotation, and binop-operand checks, but treats unknown types (a plain function call's return, a generic parameter) as "skip the check" rather than risking a false positive. Preserve that bias — a false positive in a validator is far more costly than a missed check.

Note the current span limitation: only top-level nodes (declarations, `goto`, `perform`, `send`, `spawn`) carry real spans. Expression nodes fall back to default spans, so a diagnostic on a subexpression will point at the wrong place. That's tracked in issue #46; don't build a feature that depends on precise expression spans without fixing it first.

## Build and test

```bash
cargo build --workspace
cargo test --workspace --all-targets --all-features
cargo test -p gust-lang --test codegen_backends      # the compile-the-output suite
cargo test -p gust-lang -- test_name

cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

**Examples are excluded from the workspace** (`exclude = ["examples/*"]`), so they need explicit manifests:

```bash
cargo test --manifest-path examples/event_processor/Cargo.toml
cargo test --manifest-path examples/microservice/Cargo.toml
cargo test --manifest-path examples/workflow_engine/Cargo.toml
```

A change to codegen that breaks an example will pass `cargo test --workspace` and fail CI. Run the example tests before pushing codegen changes.

## Mutation testing

The config **must** live at `.cargo/mutants.toml` — a `mutants.toml` at the repo root is silently ignored, with no warning.

```bash
cargo mutants --file gust-lang/src/validator.rs    # logic-heavy, best value
cargo mutants --shard 1/8                          # sample the workspace
```

Reading the report is the skill. A surviving mutant is either a real coverage gap (add a test) or an equivalent mutant that cannot change observable behavior (document why and accept it). Chasing 100% by suppressing the second kind is theatre; the value is being forced to articulate why each survivor is safe.

Keep runs scoped and `-j` modest — `codegen_backends.rs` spawns a real cargo or go build per fixture, so parallel mutants each spawning their own toolchain contend badly.

## Docs are tested

Every ```` ```gust ```` block under `docs/src/` must both parse *and* codegen for Rust and Go — `tests/docs_snippets.rs` walks the tree and asserts it. A syntactically invalid snippet in the docs fails the build, so documentation examples stay honest. Note the flip side: this only checks parse-and-generate, not that the generated code compiles or that the snippet is good advice.

## CI gates

`ci.yml` runs three jobs. The `core` job is: `cargo fmt --check`, `cargo check --workspace --all-targets`, `cargo clippy … -D warnings`, `cargo test --workspace --all-targets --all-features`, and `cargo doc --workspace --no-deps --all-features` (broken intra-doc links fail). Then example tests via explicit manifests plus a Go codegen smoke test with `go vet`. Separate jobs run `cargo llvm-cov` coverage to Codecov and `cargo audit`.

Clippy denies warnings and rustdoc denies broken links — both bite on new public API. `gust-build` has `#![warn(missing_docs)]`, so new public items there need doc comments.

## Conventions

Conventional Commits with a crate or area scope: `feat(parser):`, `fix(lsp):`, `test(codegen):`, `docs:`, `ci:`, `refactor:`, `build:`, `chore:`. Never mention Claude or AI in commit messages.

Generated files use `.g.rs` / `.g.go` (inspired by C# source generators) and are never hand-edited.

## References

- **`references/testing.md`** — the full test-file map, how to add a `codegen_backends` fixture, and what each suite is responsible for.
- **`references/backends.md`** — the five codegen backends, their asymmetries, and what to check when adding a form.
- **`references/validator.md`** — how to add a diagnostic, the check inventory, and the type-inference contract.
