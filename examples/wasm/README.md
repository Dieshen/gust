# WASM Example

> **Status: Not Yet Implemented**
>
> This example is a scaffold that does not yet compile or run.

## What This Will Demonstrate

Running Gust-generated state machines in the browser via WebAssembly. The `--target wasm` backend generates Rust code with:

- `#[wasm_bindgen]` entrypoints for JavaScript interop
- JS `Promise` wrappers for timeout transitions
- A JS adapter layer pattern for bridging effects to browser APIs

## What's Needed to Make This Work

1. A Cargo.toml configured as a `cdylib` with `wasm-bindgen` dependency
2. A build pipeline: `gust build --target wasm` → `wasm-pack build` → serve
3. `index.html` updated with actual JavaScript that loads the WASM module and drives the state machine
4. A `package.json` with proper build scripts (currently only has `npx serve .`)
5. Integration testing with a headless browser or `wasm-bindgen-test`

## Current Files

- `counter.gu` — A counter state machine (Zero/NonZero) with increment/decrement transitions
- `index.html` — Placeholder HTML (says "load wasm here", no actual WASM loading)
- `package.json` — Minimal npm config with only a serve script
