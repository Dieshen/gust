# Introduction

Gust is a type-safe state machine language that compiles to multiple runtime targets.

## Core model

A machine has:
- states
- transitions
- handlers (`on <transition>(...) { ... }`)
- optional effects and actions (`effect`, `action`, or their `async` forms)

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

## Current Focus

`v0.2.0` provides a complete end-to-end workflow for building state machines in
`.gu` and generating Rust/Go code with validation and tooling. It also adds
workflow-runtime features such as `action`, handler-safety diagnostics,
`EngineFailure`, JSON Schema generation, and `gust doctor`.

Use `gust build --target <target>` to select generated output per platform. See the Appendix for current limitations and workarounds.
