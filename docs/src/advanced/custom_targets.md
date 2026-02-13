# Custom Targets

Phase 4 adds scaffolding for non-default deployment targets.

## WASM

`gust build --target wasm ./counter.gu`

WASM output includes:
- `#[wasm_bindgen]`-annotated API surface
- JS `Promise` interop for async transition entrypoints
- effect adapter trait for JS callback plumbing

## no_std

`gust build --target nostd ./sensor.gu`

no_std output includes:
- `#![no_std]`
- `heapless` mappings (`String -> HString`, `Vec<T> -> HVec<T, N>`)
- transition methods that avoid std runtime dependencies

## C FFI

`gust build --target ffi ./door.gu`

FFI output includes:
- Rust `#[repr(C)]` state/handle types
- C header (`.g.h`) and Rust bridge (`.g.ffi.rs`)
- handle-based API with null and invalid-transition return codes

## Example snippet

```gust
machine Door {
    state Closed
    state Open

    transition open: Closed -> Open

    on open() {
        goto Open();
    }
}
```
