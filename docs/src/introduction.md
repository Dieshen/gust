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

## v0.1.0 Focus

`v0.1.0` provides a complete end-to-end workflow for building state machines in `.gu` and generating Rust/Go code with validation and tooling.

Use `gust build --target <target>` to select generated output per platform. See the Appendix for current limitations and workarounds.
