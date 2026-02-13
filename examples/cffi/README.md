# C FFI Example

## Build generated FFI

```bash
gust build --target ffi door.gu
```

This emits:
- `door.g.ffi.rs`
- `door.g.h`

## C side

`main.c` shows the expected include pattern for the generated header.
