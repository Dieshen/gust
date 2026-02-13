# Introduction

Gust is a type-safe state machine language that compiles to multiple runtime targets.

## Core model

A machine has:
- states
- transitions
- handlers (`on <transition>(...) { ... }`)
- optional effects (`effect` or `async effect`)

```gust
machine OrderFlow {
    state Pending(id: String)
    state Confirmed(id: String)
    state Failed(id: String, reason: String)

    transition confirm: Pending -> Confirmed | Failed

    async effect validate_payment(order_id: String) -> Result<String, String>

    async on confirm() {
        let result = perform validate_payment(id);
        match result {
            Ok(msg) => {
                goto Confirmed(id);
            }
            Err(err) => {
                goto Failed(id, err);
            }
        }
    }
}
```

## Phase 4 focus

Phase 4 extends Gust with:
- generic machines (`machine Cache<T> { ... }`)
- reusable stdlib patterns (`Saga<T>`, `Retry<T>`, etc.)
- additional targets (`wasm`, `nostd`, `ffi`)
- ecosystem docs/examples/templates

Use `gust build --target <target>` to select generated output per platform.
