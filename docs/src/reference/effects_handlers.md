# Effects and Handlers

Effects, actions, and handlers are the boundary between a Gust machine and the
host application that runs generated code.

Use this page as the language reference. For replay and checkpointing design,
see [Workflow Runtime Integration](../guides/workflow_runtime.md).

## Effects

An `effect` declares an operation that the generated machine can call through
the host application's effects interface.

```gust
machine OrderChecks {
    state Pending(order_id: String)
    state Accepted(order_id: String, token: String)
    state Rejected(order_id: String, reason: String)

    transition validate: Pending -> Accepted | Rejected

    effect validate_order(order_id: String) -> Result<String, String>

    on validate(ctx: ValidateCtx) {
        let result = perform validate_order(ctx.order_id);
        match result {
            Ok(token) => {
                goto Accepted(ctx.order_id, token);
            }
            Err(reason) => {
                goto Rejected(ctx.order_id, reason);
            }
        }
    }
}
```

Effects are assumed to be replay-safe or idempotent. Typical examples are
calculating a value, reading configuration, validating an input, or querying a
service where repeated calls are acceptable for the surrounding runtime.

Effects can be asynchronous:

```gust
machine AsyncCheck {
    state Pending(id: String)
    state Done(token: String)

    transition validate: Pending -> Done

    async effect fetch_token(id: String) -> String

    async on validate(ctx: ValidateCtx) {
        let token = perform fetch_token(ctx.id);
        goto Done(token);
    }
}
```

## Actions

An `action` has the same signature shape as an `effect`, but it marks an
operation as non-idempotent or externally visible.

```gust
machine Approval {
    state Waiting(step: String)
    state Rejected(step: String, reason: String)

    transition reject: Waiting -> Rejected

    effect normalize_reason(reason: String) -> String
    action notify_rejection(step: String, reason: String) -> String

    on reject(ctx: RejectCtx, reason: String) {
        let cleaned = perform normalize_reason(reason);
        let receipt = perform notify_rejection(ctx.step, cleaned);
        goto Rejected(ctx.step, receipt);
    }
}
```

Use `action` for work such as charging a card, sending an email, publishing a
webhook, or recording an externally visible decision. Replay-aware runtimes can
use the action marker to checkpoint before execution and restore the recorded
result during replay instead of running the operation again.

The validator emits warnings for two unsafe handler shapes:

- More than one `action` on a single code path.
- Any side-effectful step after an `action` on the same path.

Branches are analyzed independently, so different actions in sibling `if` or
`match` branches are allowed when only one branch can run.

## Handlers

A handler is an `on <transition>(...) { ... }` block. It defines the code that
runs when the host application invokes a generated transition method.

```gust
machine Shipment {
    state Charged(order_id: String)
    state Shipped(order_id: String, tracking: String)
    state Failed(order_id: String, reason: String)

    transition ship: Charged -> Shipped | Failed

    effect create_shipment(order_id: String) -> Result<String, String>

    on ship(ctx: ShipCtx) {
        let result = perform create_shipment(ctx.order_id);
        match result {
            Ok(tracking) => {
                goto Shipped(ctx.order_id, tracking);
            }
            Err(reason) => {
                goto Failed(ctx.order_id, reason);
            }
        }
    }
}
```

Handler parameters are explicit. By convention, examples use a transition
context parameter such as `ctx: ShipCtx` when the handler needs fields from the
current state. Additional event data can be passed as normal parameters.

Handlers usually terminate each reachable branch with `goto <State>(...)`.
The validator warns when declared transitions have no handler or when handler
branches can fall through without transitioning.

## `perform`

Use `perform` to call either an effect or an action:

```gust
machine Audit {
    state Ready
    state Done(record_id: String)

    transition record: Ready -> Done

    effect build_payload() -> String
    action write_audit(payload: String) -> String

    on record() {
        let payload = perform build_payload();
        let record_id = perform write_audit(payload);
        goto Done(record_id);
    }
}
```

`perform` can appear as a statement or inside an expression. When it appears in
a `let` binding, the validator checks the declared return type when enough type
information is available.

## Code Generation

Generated Rust and Go code exposes one effects interface per machine. Both
`effect` and `action` declarations become methods on that interface. Action
methods are marked in generated comments so host runtimes can preserve the
semantic distinction.

For example, a Rust target generates a trait whose methods are implemented by
the host application. A Go target generates an interface with the same role.
The generated transition method receives that implementation and calls the
declared methods while executing the handler body.

## Choosing `effect` or `action`

Use `effect` when repeating the operation is safe for the runtime's semantics.
Use `action` when repeating the operation could double-charge, duplicate a
notification, publish a second event, or otherwise change the outside world.
