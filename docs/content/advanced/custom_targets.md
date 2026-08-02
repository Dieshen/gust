---
title: "Custom Targets"
description: "The three specialist backends beyond Rust and Go — WebAssembly, no_std, and C FFI — what they actually emit, and the honest answer on adding a target of your own."
type: reference
---

# Custom Targets

Beyond `rust` and `go`, `gust build` accepts three more targets: `wasm`, `nostd`, and `ffi`. They exist for places the two production backends cannot go — a browser, a microcontroller, a C program.

They are also considerably less complete than the production backends, in a way that is easy to miss because the output compiles cleanly. This page says what each one emits, and then answers the question the page title invites: what happens when the target you want is not one of these.

::: callout warning "These three emit state machines, not behaviour"
Only the Rust and Go backends lower handler bodies. The `wasm`, `nostd`, and `ffi` backends emit the state graph — the states, the transition guards, and the state changes — and drop `perform`, `send`, and `spawn` entirely. Read the sections below before assuming a machine's logic survives.
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

## WebAssembly

```bash
gust build gate.gu --target wasm
```

Emits `gate.g.wasm.rs`: Rust annotated for `wasm-bindgen`, meant to be compiled for `wasm32-unknown-unknown` and consumed from JavaScript.

```rust "gate.g.wasm.rs"
// Generated for wasm32 target
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;
use js_sys::{Array, Promise};

pub trait GustWasmEffectAdapter {
    fn call_effect(&self, name: &str, args: Array) -> Promise;
}

#[wasm_bindgen]
#[derive(Clone, Copy)]
#[repr(u32)]
pub enum GateState {
    Closed = 0,
    Open = 1,
}

#[wasm_bindgen]
pub struct Gate {
    state: GateState,
}

#[wasm_bindgen]
impl Gate {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Gate {
        Self { state: GateState::Closed }
    }

    #[wasm_bindgen(js_name = state)]
    pub fn state(&self) -> u32 {
        self.state as u32
    }

    #[wasm_bindgen(js_name = open)]
    pub fn open(&mut self) -> Result<(), JsValue> {
        if self.state as u32 != GateState::Closed as u32 {
            return Err(JsValue::from_str("invalid transition"));
        }
        self.state = GateState::Open;
        Ok(())
    }
}
```

Compare that to the Rust output for the same machine and the gaps are stark.

**State fields are gone.** `#[wasm_bindgen]` enums are C-style, so `Closed(id: String)` becomes the bare discriminant `Closed = 0`. The machine carries a state and nothing else.

**Handler bodies are gone.** `perform authorise(...)` does not appear. `GustWasmEffectAdapter` is declared at the top of every file as a place to hang JavaScript callbacks, but nothing the emitter produces ever calls it — wiring effects to JS is left to you, outside the generated file.

**A transition with a `timeout` becomes a `Promise`.** Those methods get a `js_name` suffixed with `Async` and return `future_to_promise(...)`, so JavaScript awaits them. The timeout duration itself is not enforced by the generated code.

Dependencies for the consuming crate: `wasm-bindgen`, `wasm-bindgen-futures`, and `js-sys`.

### Generics are not expressible on this target

`#[wasm_bindgen]` rejects type parameters outright — *"structs with `#[wasm_bindgen]` cannot have lifetime or type parameters currently"*. A generic machine therefore has no valid representation here, whatever the emitter does. This is a limit of `wasm-bindgen`, not a Gust defect to be fixed, and Gust's backend test suite lists generic fixtures as unsupported for `wasm` rather than pretending otherwise.

If you need a generic machine in the browser, make it concrete first.

## no_std

```bash
gust build gate.gu --target nostd
```

Emits `gate.g.nostd.rs` for embedded and other allocation-constrained targets.

```rust "gate.g.nostd.rs"
#![no_std]
extern crate alloc;
use heapless::{String as HString, Vec as HVec};

pub enum GateState {
    Closed {
        id: HString<64>,
    },
    Open {
        id: HString<64>,
        opened_by: HString<64>,
    },
}

pub struct Gate {
    pub state: GateState,
}

impl Gate {
    pub fn new(id: HString<64>) -> Self {
        Self { state: GateState::Closed { id } }
    }

    pub fn open(&mut self, id: HString<64>, opened_by: HString<64>) -> Result<(), &'static str> {
        match &self.state {
            GateState::Closed { .. } => {
                self.state = GateState::Open { id, opened_by };
                Ok(())
            }
            _ => Err("invalid transition"),
        }
    }
}
```

This backend keeps more than WASM does — state fields survive — but it is still a skeleton.

**Collections become fixed-capacity `heapless` types.** `String` maps to `HString<64>` and `Vec<T>` to `HVec<T, 16>`. Those capacities are hard-coded in the emitter, not derived from anything you write in the `.gu`: 64 bytes per string and 16 elements per vector, whether that is generous or nowhere near enough. `Option` and `Result` pass through unchanged.

**There is no effects trait.** `authorise` does not appear anywhere in the output. Nothing performs it.

**Transitions take the target state's fields as parameters.** Since no handler body runs, the values that would have been computed there have to come from the caller instead. `open` takes `id` and `opened_by` directly, which is why its signature looks nothing like the Rust backend's `open(&mut self, effects: &impl GateEffects)`.

**Errors are `&'static str`.** No `thiserror`, no error enum — nothing that needs `std`.

The one dependency is `heapless`.

## C FFI

```bash
gust build gate.gu --target ffi
```

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

**State fields are dropped, as in WASM.** The C enum is discriminants only, and `gate_new()` takes no arguments even though `Closed` declares an `id`.

**Handler bodies are dropped.** No effects cross the boundary.

**The two files must stay in step.** They are produced together by one command; regenerate both or neither. A header that has drifted from its source lies about the ABI, and C will not notice.

The generated `extern "C"` functions have no dependencies beyond `core::ffi`.

## Adding a target of your own

There is no plugin system. Gust does not load backends from a shared library, does not discover them from a manifest key, and has no registration hook. Every emitter is compiled into `gust-lang`, and the set of names `--target` accepts is a hard-coded match. Nothing in the CLI, the manifest schema, or the `build.rs` helper is extensible at runtime.

That leaves two honest routes.

### Fork the compiler

Add a `codegen_<target>.rs` to `gust-lang`, wire it into the `--target` match in `gust-cli`, and — if you want it reachable from a `build.rs` — extend `gust-build`'s target enum. Then wire it into the manifest schema if you want `gust generate` to reach it, which is a third, separate place.

This is a real fork with real maintenance cost. Every language feature added upstream has to be handled by your backend or it silently emits nothing for the new form — which is exactly how three of the shipped backends ended up emitting output that no compiler had ever accepted. If you take this route, mirror the upstream practice of compiling your emitter's output with its real toolchain in a test, not asserting on the emitted strings.

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

You get the same `Program` the built-in backends receive, and you own your generator entirely: your release cycle, your test suite, no fork to rebase. The cost is that AST changes upstream are breaking changes for you, and the AST is not covered by a stability promise.

If Rust is not where you want to work, two other surfaces expose the same information without linking against the compiler:

- **`gust schema`** emits JSON Schema for a machine's states and declared types. Enough to generate data structures in another language, though it says nothing about transitions.
- **The `gust_parse` tool on the MCP server** returns the AST as JSON — machines, states, transitions, effects with their `effect`/`action` kind. That is the full model in a language-neutral form, which makes it the most practical starting point for a generator written outside Rust.

::: callout tip "Consider whether you need a target at all"
Generated Rust and Go are ordinary source files. Wrapping them by hand — a C ABI over the Rust output, a WASM binding over your own struct — costs one small adapter per machine and keeps every handler body, effect, and channel that the specialist backends drop. Reach for a new backend when the wrapper stops scaling, not before.
:::

## Next steps

- [Code Generation](codegen.md) — the pipeline, and the Rust and Go output these targets are measured against.
- [Contract Packages](contract_packages.md) — note that `wasm`, `nostd`, and `ffi` are not manifest targets, so they need their own `gust build` invocation.
