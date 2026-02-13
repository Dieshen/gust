# WASM Example

## Build

1. Generate wasm-oriented Rust from Gust:

```bash
gust build --target wasm counter.gu
```

2. Feed generated output into your wasm-bindgen crate setup.

## Notes

- Generated code includes `#[wasm_bindgen]` entrypoints.
- Timeout transitions are represented with JS `Promise` wrappers.
- Effects are expected to be bridged through a JS adapter layer.
