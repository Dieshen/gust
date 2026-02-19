# C FFI Example

> **Status: Not Yet Implemented**
>
> This example is a scaffold that does not yet compile or run.

## What This Will Demonstrate

Calling Gust-generated state machines from C code via FFI. The Gust compiler's `--target ffi` backend generates:

- `door.g.ffi.rs` — Rust FFI exports with `#[no_mangle]` and `extern "C"` functions
- `door.g.h` — C header file for the generated API

`main.c` shows the intended include and call pattern for driving a state machine from C.

## What's Needed to Make This Work

1. A working `gust build --target ffi` that generates both `.g.ffi.rs` and `.g.h` files
2. A Cargo.toml for building the Rust side as a `cdylib` or `staticlib`
3. A Makefile or build script that compiles Rust → library, then links with `main.c`
4. Integration testing to verify C code can create machines, trigger transitions, and inspect state

## Current Files

- `door.gu` — A simple door state machine (Closed/Open) used as the FFI test case
- `main.c` — Placeholder C code that includes the expected generated header
- `Makefile` — Placeholder build script (does not work yet)
