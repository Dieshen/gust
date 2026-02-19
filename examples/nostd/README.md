# no_std Example

> **Status: Not Yet Implemented**
>
> This example is a scaffold that does not yet compile or run.

## What This Will Demonstrate

Running Gust-generated state machines in `#![no_std]` environments (embedded systems, kernels, bare-metal). The `--target nostd` backend generates Rust code that:

- Uses no heap allocation (no `String`, `Vec`, `Box`)
- Maps dynamic containers to `heapless` equivalents where possible
- Emits `#![no_std]` prelude

## What's Needed to Make This Work

1. A valid Cargo.toml with `no_std`-compatible dependencies (no `gust-runtime` which requires `std`)
2. A `src/` directory with `lib.rs` or `main.rs` that includes the generated code
3. A build.rs or manual step to generate `sensor.g.nostd.rs` from `sensor.gu`
4. A target-specific build configuration (e.g., `thumbv7em-none-eabihf` for ARM Cortex-M)
5. Verification that generated code compiles without `std`

## Current Files

- `sensor.gu` — A simple sensor state machine (Idle/Reading) for embedded use
- `Cargo.toml` — Placeholder (structurally invalid, missing `src/` directory)
