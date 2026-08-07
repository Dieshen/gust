---
title: "Custom Targets"
description: "The one specialist backend beyond Rust and Go, the two that were removed in 1.0 and what replaced them, and the honest answer on adding a target of your own."
type: reference
---

# Custom Targets

Beyond `rust` and `go`, `gust build` accepts one more target: `ffi`, behind an explicit `--unstable-ffi` opt-in.

Two others — `wasm` and `nostd` — were removed in 1.0. This page covers what `ffi` emits, why the other two are gone and what to use instead, and then answers the question the page title invites: what happens when the target you want is not one of these.

::: callout warning "`ffi` emits a state machine, not behaviour"
Only the Rust and Go backends lower handler bodies. The `ffi` backend emits the state graph — the states, the transition guards, and the state changes — and drops `perform`, `send`, and `spawn` entirely. Read the section below before assuming a machine's logic survives.
:::

## The reference machine

Every example on this page is generated from one source file:

```gust
machine Gate {
    state Closed(id: String)
    state Open(id: String, opened_by: String)

    transition open: Closed -> Open

    effect authorise(id: String) -> String

    on open(ctx: OpenCtx) {
        let who = perform authorise(ctx.id);
        goto Open(ctx.id, who);
    }
}
```

For what the `rust` and `go` targets do with it, see [Code Generation](codegen.md).

## C FFI

```bash
gust build gate.gu --target ffi --unstable-ffi
```

::: callout warning "Outside the 1.0 stability promise"
`--unstable-ffi` is not ceremony. The `.g.h` header is generated from the same AST as the Rust half, but **no CI job compiles it** — verifying it would need a C toolchain in the pipeline. Rather than freeze an unverified artefact into the 1.0 promise, this backend is explicitly excluded from it, and its output shape may change within 1.x. Opting in is an acknowledgement that an upgrade can break the header.
:::

Emits two files: `gate.g.ffi.rs` and `gate.g.h`. The Rust side is an opaque handle with `extern "C"` entry points; the header declares the same ABI to C.

```c "gate.g.h"
#ifndef GUST_FFI_H
#define GUST_FFI_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum GateState {
    GATE_STATE_CLOSED = 0,
    GATE_STATE_OPEN = 1,
} GateState;

typedef struct GateHandle GateHandle;
GateHandle* gate_new(void);
void gate_free(GateHandle* handle);
GateState gate_state(const GateHandle* handle);
int gate_open(GateHandle* handle);

#ifdef __cplusplus
}
#endif

#endif
```

```rust "gate.g.ffi.rs"
#[no_mangle]
pub unsafe extern "C" fn gate_open(handle: *mut GateHandle) -> c_int {
    if handle.is_null() {
        return -1;
    }
    if (*handle).state != GateState::Closed {
        return -2;
    }
    (*handle).state = GateState::Open;
    0
}
```

The conventions are worth committing to memory, because the header does not document them:

- `0` — the transition succeeded.
- `-1` — the handle was null.
- `-2` — the transition is not legal from the current state.

**State fields are dropped.** The C enum is discriminants only, and `gate_new()` takes no arguments even though `Closed` declares an `id`.

**Handler bodies are dropped.** No effects cross the boundary.

**The two files must stay in step.** They are produced together by one command; regenerate both or neither. A header that has drifted from its source lies about the ABI, and C will not notice.

The generated `extern "C"` functions have no dependencies beyond `core::ffi`.

## Targets removed in 1.0 {#removed}

1.0 is a stability promise, and the worst thing to freeze into one is a backend whose output compiles without implementing the source machine. Both of these did exactly that, which is why they were removed rather than fixed.

The backend test matrix compiles every fixture's output with that backend's real toolchain. That catches malformed output, and it caught a great deal — but **compiling proves well-formedness, not behaviour**, so it could not catch either of these.

### `wasm`

The emitter produced a state-name tracker. State payload fields were dropped, since `#[wasm_bindgen]` enums are C-style — `Closed(id: String)` became the bare discriminant `Closed = 0`. Every handler body was dropped. No effect was ever invoked: the `GustWasmEffectAdapter` trait it declared at the top of each file was referenced by nothing the emitter produced. A multi-target transition always took its first target.

`#[wasm_bindgen]` also rejects type parameters outright — *"structs with `#[wasm_bindgen]` cannot have lifetime or type parameters currently"* — so no generic machine could ever have a valid representation there, whatever the emitter did.

**Use instead:** the **Rust** backend, compiled to `wasm32`.

```bash
gust build gate.gu --target rust
```

Generated Rust is ordinary Rust. Add it to a `cdylib` crate, implement `GateEffects` against your JavaScript bindings, and build for `wasm32-unknown-unknown`. You keep every handler body, every effect, and the real state payloads — none of which survived the old backend. Generics work, because nothing forces the machine itself through `#[wasm_bindgen]`; only your hand-written wrapper crosses that boundary, and you choose its shape.

### `nostd`

Emitted the state enum, the user type declarations, and transitions that construct the target state — a typed state container rather than a port of the runtime. **No handler bodies and no effects trait**, so transitions took the target state's fields as parameters, because nothing computed them. Collections became fixed-capacity `heapless` types with capacities hard-coded in the emitter (64 bytes per string, 16 elements per vector) rather than derived from anything in the `.gu`.

**Use instead:** the **Rust** backend, adapted in the host. The generated code depends on `serde` and `thiserror`; on a target where those are unavailable, the state enum and transition methods are small enough to wrap by hand, and you decide the capacities.

### Cleaning up after the removal

Leftover `.g.wasm.rs` and `.g.nostd.rs` files cannot be regenerated. `gust doctor` deliberately does not list them as freshness candidates — telling you a file is "stale, regenerate" would send you after a flag that no longer exists. Delete them.

## Adding a target of your own

There is no plugin system. Gust does not load backends from a shared library, does not discover them from a manifest key, and has no registration hook. Every emitter is compiled into `gust-lang`, and the set of names `--target` accepts is a hard-coded match. Nothing in the CLI, the manifest schema, or the `build.rs` helper is extensible at runtime.

That leaves two honest routes.

### Fork the compiler

Add a `codegen_<target>.rs` to `gust-lang`, wire it into the `--target` match in `gust-cli`, and — if you want it reachable from a `build.rs` — extend `gust-build`'s target enum. Then wire it into the manifest schema if you want `gust generate` to reach it, which is a third, separate place.

This is a real fork with real maintenance cost. Every language feature added upstream has to be handled by your backend or it silently emits nothing for the new form — which is exactly how the removed backends ended up shipping output that implemented none of the source. If you take this route, mirror the upstream practice of compiling your emitter's output with its real toolchain in a test rather than asserting on emitted strings. Then go one further than upstream did and assert on *behaviour*, because compiling is the bar that both removed backends cleared.

### Build on `gust-lang` as a library

Usually the better option. `gust-lang` is a published crate with a public AST and a public parser:

```rust
use gust_lang::{parse_program, validate_program};

let source = std::fs::read_to_string("order.gu").expect("readable");
let program = parse_program(&source).expect("parses");
let report = validate_program(&program, "order.gu", &source);

// `program.uses`, `program.types`, `program.channels`, `program.machines`
// are the whole model. Walk them and emit whatever you like.
```

You get the same `Program` the built-in backends receive, and you own your generator entirely: your release cycle, your test suite, no fork to rebase. The cost is that AST changes upstream are breaking changes for you, and the AST is not covered by the [stability promise](../appendix/stability.md).

If Rust is not where you want to work, two other surfaces expose the same information without linking against the compiler:

- **`gust schema`** emits JSON Schema for a machine's states and declared types. Enough to generate data structures in another language, though it says nothing about transitions.
- **The `gust_parse` tool on the MCP server** returns the AST as JSON — machines, states, transitions, effects with their `effect`/`action` kind, and full handler bodies. That is the whole model in a language-neutral form, which makes it the most practical starting point for a generator written outside Rust.

::: callout tip "Consider whether you need a target at all"
Generated Rust and Go are ordinary source files. Wrapping them by hand — a C ABI over the Rust output, a WASM binding over your own struct — costs one small adapter per machine and keeps every handler body, effect, and channel that a specialist backend drops.

This is not a consolation prize. It is what replaced the `wasm` backend, and it produces a strictly more capable result than that backend ever did. Reach for a new backend when the wrapper stops scaling, not before.
:::

## Next steps

- [Code Generation](codegen.md) — the pipeline, and the Rust and Go output this target is measured against.
- [Stability](../appendix/stability.md) — what the 1.0 promise covers, and why `ffi` sits outside it.
- [Contract Packages](contract_packages.md) — note that `ffi` is not a manifest target, so it needs its own `gust build` invocation.
