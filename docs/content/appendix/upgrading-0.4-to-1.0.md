---
title: "Upgrading 0.4 → 1.0"
description: "Every breaking change between Gust 0.4.1 and 1.0, in the order likely to hit you."
type: guide
---

# Upgrading 0.4 → 1.0

```bash
cargo install gust-cli --locked --force   # gust 1.0
```

Bump `gust-runtime` to `1.0` in any crate consuming generated Rust. Regenerate
all `.g.rs` / `.g.go` — do not mix output from two compiler versions in one
build.

1.0 is a [stability promise](stability.md): source that compiles at 1.0 keeps
compiling across 1.x. Getting there meant making the breaking changes now
rather than discovering them later, so this release is deliberately front-loaded.

Ordered by how likely each is to hit you.

::: callout info "This page is written as the changes land"
It is not reconstructed at release time. Two 0.4.0 breaking changes reached
users with no changelog entry at all because the notes were written afterwards;
writing the guide alongside the work is the fix.
:::

---

## 1. The `wasm` and `nostd` backends are removed

`gust build --target wasm` and `--target nostd` now exit non-zero.

Both emitted output that **compiled without implementing the source machine** —
the one thing a stability promise must not be extended over. `wasm` was the
worse of the two: state payload fields dropped (`state New(id: String)` became
the bare discriminant `New = 0`), every handler body dropped, no effect ever
invoked, and a multi-target transition always took its first target. The
`GustWasmEffectAdapter` trait it declared was referenced by nothing it emitted.
`nostd` kept state fields but emitted no handler bodies and no effects trait.

Neither was caught by CI, because the backend matrix compiles each fixture's
output with its real toolchain — which proves output is well-formed, not that it
does what the source says.

### Replacing `wasm`

Compile the **Rust** backend's output to `wasm32`:

```bash
gust build order.gu --target rust
```

Add the generated file to a `cdylib` crate, implement the generated
`{Machine}Effects` trait against your JavaScript bindings, and build for
`wasm32-unknown-unknown`.

This is strictly more capable than the removed backend. You keep handler bodies,
effects, and real state payloads — none of which survived before. Generic
machines work too, because nothing forces the machine itself through
`#[wasm_bindgen]`; only your hand-written wrapper crosses that boundary and you
choose its shape. The old backend could never support generics at all, since
`#[wasm_bindgen]` rejects type parameters outright.

### Replacing `nostd`

Use the Rust backend and adapt it in the host. Generated Rust depends on `serde`
and `thiserror`; where those are unavailable, the state enum and transition
methods are small enough to wrap by hand — and you pick your own capacities
rather than inheriting the emitter's hard-coded 64-byte strings and 16-element
vectors.

### Cleaning up

Leftover `.g.wasm.rs` and `.g.nostd.rs` files cannot be regenerated. Delete
them. `gust doctor` deliberately no longer lists them as freshness candidates,
because reporting one as "stale, regenerate" would send you after a flag that no
longer exists.

### Library consumers

`WasmCodegen`, `NoStdCodegen`, `Target::Wasm`, and `Target::NoStd` are gone from
`gust-lang` and `gust-build`.

## 2. `--target ffi` requires `--unstable-ffi`

```bash
gust build gate.gu --target ffi --unstable-ffi
```

The `.g.h` header is generated from the same AST as the Rust half, but **no CI
job compiles it** — verifying it would need a C toolchain in the pipeline.
Rather than freeze an unverified artefact into the 1.0 promise, this backend is
explicitly excluded from it and its output shape may change within 1.x.

The flag is the acknowledgement that an upgrade can break your header. Nothing
about the generated output changed in 1.0; only the opt-in is new.

`ffi` is still not a `gust.toml` manifest target, so it continues to need its
own `gust build` invocation.

---

## Next steps

- [Stability](stability.md) — what the 1.0 promise covers and what it does not.
- [Custom Targets](../advanced/custom_targets.md) — the surviving specialist
  backend, and what it takes to add one of your own.
- [Changelog](changelog.md) — the same changes organised by release mechanics.
