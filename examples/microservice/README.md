# Microservice Example

A working example demonstrating multi-machine coordination with three Gust state
machines: `OrderMachine`, `PaymentMachine`, and `SupervisorMachine`.

## What It Shows

- Three independent `.gu` machines operating in a single Rust binary
- Cross-machine coordination: order processing integrates with a payment machine
- Effect traits for deterministic, testable business logic
- Conditional branching (success vs. failure paths)
- Invalid transition enforcement

## Machines

### OrderMachine (`src/order.gu`)

Drives the lifecycle of a single order:

```
Pending -> Validated -> Charged -> Shipped
       \-> Failed
```

- `validate`: calls `calculate_total` effect; routes to `Failed` when total is zero
- `charge`: calls `process_payment` effect
- `ship`: calls `create_shipment` effect; returns a tracking number
- `fail`: explicit failure transition from `Pending`

### PaymentMachine (`src/payment.gu`)

Manages the two-phase charge sequence:

```
Awaiting -> Processing -> Settled
         \-> Declined
```

- `initiate`: calls `initiate_charge` effect; routes to `Declined` when amount is zero
- `confirm`: calls `confirm_charge` effect; produces a `PayReceipt`

### SupervisorMachine (`src/supervisor.gu`)

Monitors system health:

```
Running -> Degraded -> Running
                   \-> Shutdown
```

- `report_failure`: records a failure count and enters degraded state
- `recover`: clears degraded state and returns to running
- `shutdown`: terminal state — no further transitions are valid

## Running

```sh
cargo run
```

Sample output:

```
=== Microservice Example: Multi-Machine Coordination ===

-- Scenario 1: Full order lifecycle (happy path) --
  Order initial state:   Pending { order: Order { id: "ord-001", ... } }
  After validate:        Validated { ..., total: Money { amount: 3000, currency: "USD" } }
  Payment initial state: Awaiting { amount: PayMoney { amount: 3000, ... } }
  After initiate:        Processing { tx_id: "charge-3000", ... }
  After confirm:         Settled { receipt: PayReceipt { transaction_id: "charge-3000", ... } }
  After order charge:    Charged { ..., receipt: Receipt { transaction_id: "txn-3000", ... } }
  After ship:            Shipped { ..., tracking: "SHIP-ORD-001-001" }
  Order ord-001 shipped with tracking: SHIP-ORD-001-001
  Supervisor state:      Running (no failures)

-- Scenario 2: Invalid order -> validation failure --
  After validate (qty=0): Failed { reason: "invalid order total" }
  Supervisor after failure: Degraded { failure_count: 1 }
  Payment machine (amt=0): Declined { reason: "amount must be positive" }

-- Scenario 3: Supervisor tracks failures and recovers --
  Initial:               Running
  After 1st failure:     Degraded { failure_count: 1 }
  After recovery:        Running
  After 2nd failure:     Degraded { failure_count: 2 }
  After shutdown:        Shutdown
  Invalid recover from Shutdown: invalid transition 'recover' from state 'Shutdown'
```

## Testing

```sh
cargo test
```

14 tests covering:

- Full happy-path order lifecycle with field value assertions
- Zero-quantity order routing to `Failed`
- Explicit `fail` transition
- Invalid transitions returning `InvalidTransition` errors
- Payment happy path and zero-amount decline
- Supervisor failure, recovery, and terminal shutdown
- Multi-machine coordination: order + payment working together

## Structure

```
examples/microservice/
  src/
    order.gu          # OrderMachine definition
    payment.gu        # PaymentMachine definition
    supervisor.gu     # SupervisorMachine definition
    effects.rs        # MicroserviceEffects implementing all effect traits
    main.rs           # Binary: includes generated code, demonstrates 3 scenarios
  tests/
    integration_test.rs  # 14 integration tests
  build.rs            # Calls gust_build::compile_gust_files()
  Cargo.toml
```

## Key Pattern: Multiple Machines in One Crate

When including multiple generated files in the same Rust file, each must be
wrapped in its own module to avoid duplicate `use serde::...` import errors:

```rust
pub mod order_machine {
    include!("order.g.rs");
}
pub mod payment_machine {
    include!("payment.g.rs");
}
pub use order_machine::*;
pub use payment_machine::*;
```

This is the idiomatic pattern for multi-machine Gust crates.
