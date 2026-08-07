---
title: "Stability"
description: "What the 1.0 promise covers, what it deliberately does not, and how changes reach you."
type: reference
---

# Stability

1.0 is a promise, not a milestone. This page says exactly what is promised, because a promise whose scope is vague is worth less than a narrow one stated plainly.

## The promise

**`.gu` source that compiles at 1.0 keeps compiling across 1.x**, and generated code keeps the same public shape.

Concretely, three surfaces are covered.

### 1. Surface syntax

Every form accepted by `grammar.pest` at 1.0 is accepted by every 1.x release. New forms may be added; existing ones are not removed, and their meaning does not change.

This includes the shapes it is tempting to treat as incidental: the `ctx` parameter convention, the positional zip of `goto` arguments onto target-state fields, a machine's constructor being the fields of its *first* state, and `perform` as an expression as well as a statement.

### 2. Generated public API

For the `rust` and `go` backends:

- state enum name and variants, and the fields on each variant
- machine struct name, `new`, and the state accessor
- transition method names and signatures
- the `{Machine}Effects` trait / interface, and every method on it
- the `{Machine}Error` enum and its variants
- the supervision trait and the `{MACHINE}_SUPERVISION` strategy table
- serialization shape — what `to_json` produces and `from_json` accepts

If your host code names it, it is covered.

### 3. JSON Schema output

`gust schema` output stays structurally compatible: `$defs` keys, the object/`oneOf` shapes for structs and enums, per-state definitions, and the `{Machine}_State` union.

## What is not covered

Stated as explicitly as the promise itself, because an unstated exclusion is a broken promise waiting to happen.

**Generated internals.** How a transition body is lowered — which locals it binds, whether a field is cloned or moved, the order of match arms, formatting and comments. If your host code does not name it, it can change.

**Diagnostic wording.** Error and warning text, ordering, and the did-you-mean suggestions. Diagnostics improve; pinning their text would freeze that. Parse them at your peril.

**Formatter output.** `gust fmt` may change whitespace, blank-line placement, and wrapping within 1.x. It will not change the *meaning* of what it formats.

**The MCP JSON AST.** `gust_parse` output tracks the AST, and the AST tracks the language. New node kinds and new fields appear as the language grows. Consume it defensively.

**The `gust-lang` Rust API.** `Program`, `Expr`, `Statement` and friends are public so that you can build on them, but they are not frozen. A new language feature is a new AST node, and that is a breaking change for anything matching exhaustively.

**The `ffi` backend.** Behind `--unstable-ffi`. Its `.g.h` header is generated from the same AST as the Rust half, but no CI job compiles it — verifying it would need a C toolchain in the pipeline. Rather than freeze an unverified artefact, it is excluded, and its shape may change within 1.x. The flag is your acknowledgement of that.

## Semantic versioning, applied to a compiler

A compiler has two audiences — the person writing `.gu` and the person consuming generated code — and a change can be invisible to one and breaking for the other. The rule is that **the more severe classification wins**.

| Change | Version |
|---|---|
| A `.gu` that compiled no longer compiles | **major** |
| A `.gu` that compiled produces *different runtime behaviour* | **major** |
| Generated public API changes shape | **major** |
| JSON Schema output changes structurally | **major** |
| New syntax; previously-invalid source now compiles | minor |
| New validator **error** on source that used to pass | minor |
| New validator **warning** | patch |
| Generated internals change; public API identical | patch |
| Diagnostic wording, formatter whitespace | patch |

The second row is the one that has bitten this project. In 0.4.0, `goto` began ending the handler — before that it emitted a bare state assignment and execution continued, so an early `goto` inside an `if` fell through and the machine ended in whichever state the *last* assignment named. Machines that compiled before still compiled after, and did something different. **That is major**, and it shipped in a minor with no changelog entry at all.

A new validator error is minor rather than major by deliberate choice: source that newly fails validation was already producing output its host toolchain rejected. Surfacing that at `gust check` instead of at `rustc` is a fix, not a regression — but it can still stop a build, so it never lands in a patch.

## How changes reach you

**Every change to `gust-lang/src/` lands with a `CHANGELOG.md` entry.** This is enforced in CI rather than left to review, because the two 0.4.0 breaking changes that reached users unannounced were both authored, reviewed, and merged by people who knew about them.

**Breaking changes are also written into the upgrade guide as they land**, not reconstructed at release time. Reconstruction is how they went missing.

**Deprecation.** A form scheduled for removal warns for at least one minor release before a major removes it, and the warning names the replacement. Nothing is removed in 1.x.

## What backs the promise

Three mechanisms, because intent is not a mechanism:

- **The `.gu` compatibility corpus** — a locked set of sources that must keep compiling, each tagged with the release that introduced it. Includes every stdlib machine and every example.
- **Golden output tests** — codegen is deterministic, so generated output is snapshotted. A diff is not a failure to be silenced; it is a decision that needs a changelog entry.
- **The backend matrix** (`codegen_backends.rs`) — every fixture's output is compiled with that backend's real toolchain.

The third has a known limit worth naming here. **Compiling proves output is well-formed, not that it does what the source says.** Two backends passed that bar for their entire existence while discarding the machine's behaviour entirely; they were removed in 1.0 rather than frozen into this promise. Golden tests exist partly to cover that gap, and behaviour still needs a human reading output against source.

## Next steps

- [Upgrading 0.4 → 1.0](upgrading-0.4-to-1.0.md) — the breaking changes 1.0 itself makes.
- [Known Limitations](known_limitations.md) — where Gust stops, stated bluntly.
- [Changelog](changelog.md) — every change, by release.
