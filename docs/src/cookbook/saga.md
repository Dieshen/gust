# Saga

The Saga pattern coordinates multi-step workflows where each completed step can be compensated.

## Model

A Saga machine typically includes:
- forward execution state (`Executing`)
- compensation state (`Compensating`)
- terminal success/failure states

In `gust-stdlib/saga.gu`, compensation walks completed steps in reverse order.

## Usage sketch

```gust
machine BookingWorkflow {
    state Planning(steps: Vec<String>)
    state Running(saga: Saga<String>)
    state Complete
    state Failed(reason: String)

    transition start: Planning -> Running
    transition tick: Running -> Running | Complete | Failed

    on start() {
        goto Running(saga);
    }

    on tick() {
        goto Running(saga);
    }
}
```

## Operational guidance

- make forward steps idempotent where possible
- keep compensation side effects explicit and auditable
- include enough metadata in step payloads to perform compensation safely
