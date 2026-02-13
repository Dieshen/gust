# Phase 2 Errors/Validation + CLI Tooling Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add human-friendly parse/validation diagnostics and new CLI commands (`init`, `fmt`, `check`, `diagram`) with tests and clean verification.

**Architecture:** Keep parser as the single source of syntax truth and layer typed diagnostics on top. Add a validator pass in `gust-lang` for semantic warnings/errors. Add a formatter module in `gust-lang` for deterministic output. Wire CLI commands in `gust-cli` to parser+validator+formatter APIs.

**Tech Stack:** Rust workspace, `pest` parser, `clap` CLI, `strsim` (suggestions), `colored` (terminal output).

---

### Task 1: Add diagnostics and validator modules in `gust-lang`

**Files:**
- Create: `gust-lang/src/error.rs`
- Create: `gust-lang/src/validator.rs`
- Modify: `gust-lang/src/lib.rs`
- Modify: `gust-lang/Cargo.toml`

**Step 1:** Add failing tests for duplicate state and undefined target diagnostics.
**Step 2:** Implement `GustError`, `GustWarning`, renderer with file:line:col + snippet + caret.
**Step 3:** Implement validator checks (duplicate states/transitions, undefined targets, unreachable states, unused effects).
**Step 4:** Export modules from `lib.rs`.
**Step 5:** Run `cargo test -p gust-lang`.

### Task 2: Add parser helper returning rich errors

**Files:**
- Modify: `gust-lang/src/parser.rs`
- Modify: `gust-lang/src/lib.rs`

**Step 1:** Add failing tests for parse error location extraction and suggestion helper.
**Step 2:** Implement `parse_program_with_errors(source, path)` returning `Result<Program, GustError>`.
**Step 3:** Keep existing `parse_program` for compatibility.
**Step 4:** Run `cargo test -p gust-lang`.

### Task 3: Add formatter module

**Files:**
- Create: `gust-lang/src/format.rs`
- Modify: `gust-lang/src/lib.rs`

**Step 1:** Add failing formatting/idempotence tests.
**Step 2:** Implement deterministic formatter (types/machines/states/transitions/effects/handlers).
**Step 3:** Run formatter tests and package tests.

### Task 4: Add CLI commands `init`, `fmt`, `check`, `diagram`

**Files:**
- Modify: `gust-cli/src/main.rs`
- Modify: `gust-cli/Cargo.toml`

**Step 1:** Add command variants and failing command-level tests where practical.
**Step 2:** Implement:
- `gust init <name>`
- `gust fmt <file>`
- `gust check <file>`
- `gust diagram <file> [-o output]`
**Step 3:** Wire diagnostics renderer and validator output.
**Step 4:** Run `cargo test -p gust-cli`.

### Task 5: Full verification

**Files:**
- Workspace

**Step 1:** Run `cargo test`.
**Step 2:** Run `cargo clippy --all-targets --all-features`.
**Step 3:** Confirm no regressions in existing commands.

