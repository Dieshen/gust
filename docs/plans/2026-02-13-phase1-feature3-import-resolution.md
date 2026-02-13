# Phase 1 Feature 3 (Import Resolution) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Emit user-declared `use` imports from `.gu` files into generated Rust and Go output, with tests proving correct behavior and no regressions.

**Architecture:** Keep parser/AST unchanged because `Program.uses` already exists and is populated. Extend Rust and Go codegen preludes to consume `Program` and render imports deterministically. Add integration-style codegen tests in `gust-lang/tests` that assert exact generated snippets for both targets.

**Tech Stack:** Rust 2021, `cargo test`, existing `gust-lang` parser/codegen modules.

---

### Task 1: Add Rust codegen coverage for `use` declarations

**Files:**
- Create: `gust-lang/tests/import_resolution.rs`
- Modify: `gust-lang/Cargo.toml`

**Step 1: Write the failing test**

Create `gust-lang/tests/import_resolution.rs` with a Rust-target test that:
- Parses a `.gu` source containing:
  - at least one Rust-style import like `use crate::models::Order;`
  - one machine and one type so normal generation still runs
- Runs `RustCodegen::new().generate(&program)`
- Asserts generated output contains:
  - `use crate::models::Order;`
  - the default runtime prelude imports that already existed

**Step 2: Run test to verify it fails**

Run: `cargo test -p gust-lang import_resolution_rust_emits_use_paths -- --exact`
Expected: FAIL because current `emit_prelude()` ignores `program.uses`.

**Step 3: Write minimal implementation**

No production code yet in this task. This task is only to establish failing test + harness.

**Step 4: Run test to verify it still fails for expected reason**

Run: `cargo test -p gust-lang import_resolution_rust_emits_use_paths -- --exact`
Expected: FAIL with missing expected import line.

**Step 5: Commit**

```bash
git add gust-lang/tests/import_resolution.rs gust-lang/Cargo.toml
git commit -m "test: add failing rust import resolution codegen test"
```

### Task 2: Implement Rust import emission

**Files:**
- Modify: `gust-lang/src/codegen.rs`
- Test: `gust-lang/tests/import_resolution.rs`

**Step 1: Keep existing test red**

Run: `cargo test -p gust-lang import_resolution_rust_emits_use_paths -- --exact`
Expected: FAIL before code change.

**Step 2: Write minimal implementation**

In `gust-lang/src/codegen.rs`:
- Update `generate()` to pass `program` into prelude emission.
- Change `emit_prelude()` signature from zero-arg to accept `&Program`.
- After existing built-in `use` lines, iterate `program.uses` and emit:
  - `use seg1::seg2::...;` for each `UsePath`
- Keep output stable (same ordering as source, no dedup unless required).

**Step 3: Run focused test to verify green**

Run: `cargo test -p gust-lang import_resolution_rust_emits_use_paths -- --exact`
Expected: PASS.

**Step 4: Run package tests**

Run: `cargo test -p gust-lang`
Expected: PASS.

**Step 5: Commit**

```bash
git add gust-lang/src/codegen.rs gust-lang/tests/import_resolution.rs
git commit -m "feat: emit user imports in rust codegen prelude"
```

### Task 3: Add Go codegen coverage for `use` declarations

**Files:**
- Modify: `gust-lang/tests/import_resolution.rs`
- Test: `gust-lang/src/codegen_go.rs` (no prod changes in this task)

**Step 1: Write the failing test**

Add a second test in `gust-lang/tests/import_resolution.rs` for Go target that:
- Uses `.gu` source with:
  - one Go-style import path represented via `use github::com::acme::payments;`
  - one standard type import-like path (if supported by mapper)
- Runs `GoCodegen::new().generate(&program, "testpkg")`
- Asserts Go `import (...)` block includes expected mapped string(s), e.g. `"github.com/acme/payments"`.

**Step 2: Run test to verify it fails**

Run: `cargo test -p gust-lang import_resolution_go_emits_use_paths -- --exact`
Expected: FAIL because current Go prelude is static.

**Step 3: Write minimal implementation**

No production code yet in this task.

**Step 4: Re-run test for expected failure mode**

Run: `cargo test -p gust-lang import_resolution_go_emits_use_paths -- --exact`
Expected: FAIL due to missing import line.

**Step 5: Commit**

```bash
git add gust-lang/tests/import_resolution.rs
git commit -m "test: add failing go import resolution codegen test"
```

### Task 4: Implement Go import emission

**Files:**
- Modify: `gust-lang/src/codegen_go.rs`
- Test: `gust-lang/tests/import_resolution.rs`

**Step 1: Keep Go test red**

Run: `cargo test -p gust-lang import_resolution_go_emits_use_paths -- --exact`
Expected: FAIL before code changes.

**Step 2: Write minimal implementation**

In `gust-lang/src/codegen_go.rs`:
- Update `generate()` to pass `program` into prelude emission.
- Update `emit_prelude()` signature to accept `&Program` and `package_name`.
- Merge dynamic imports from `program.uses` into import block:
  - Convert `UsePath` segments to Go path using `/` separator.
  - Keep existing required runtime imports intact.
  - Avoid duplicate imports.
- Preserve deterministic ordering.

**Step 3: Run focused tests**

Run:
- `cargo test -p gust-lang import_resolution_go_emits_use_paths -- --exact`
- `cargo test -p gust-lang import_resolution_rust_emits_use_paths -- --exact`

Expected: PASS.

**Step 4: Run full package tests**

Run: `cargo test -p gust-lang`
Expected: PASS.

**Step 5: Commit**

```bash
git add gust-lang/src/codegen_go.rs gust-lang/tests/import_resolution.rs
git commit -m "feat: emit user imports in go codegen prelude"
```

### Task 5: Verify CLI behavior remains compatible

**Files:**
- Test: `gust-cli/src/main.rs` (no code change expected)

**Step 1: Run CLI-targeted smoke checks**

Run:
- `cargo run -p gust-cli -- build examples/order_processor.gu --target rust`
- `cargo run -p gust-cli -- build examples/order_processor.gu --target go`

Expected: Both commands succeed and generated files still compile syntactically.

**Step 2: Optional guard assertion**

If needed, add grep checks for emitted import lines in output files.

**Step 3: Run workspace verification**

Run:
- `cargo test`
- `cargo check`

Expected: PASS across workspace.

**Step 4: Commit**

```bash
git add -A
git commit -m "chore: verify import resolution feature across cli and workspace"
```

