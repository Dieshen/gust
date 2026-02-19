// Integration tests for the three microservice state machines.
//
// Covers:
//   - Full order lifecycle happy path (validate -> charge -> ship)
//   - Payment machine: positive and zero amounts
//   - Order machine: zero-quantity order routes to Failed
//   - Supervisor: failure tracking, recovery, and shutdown
//   - Invalid transitions return errors
//
// Each generated file is wrapped in a module to avoid duplicate
// `use serde::...` imports when multiple machines are included together.

pub mod order_machine {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/order.g.rs"));
}
pub mod payment_machine {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/payment.g.rs"));
}
pub mod supervisor_machine {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/supervisor.g.rs"));
}

pub use order_machine::*;
pub use payment_machine::*;
pub use supervisor_machine::*;

// TestEffects provides deterministic, side-effect-free implementations for testing.
struct TestEffects;

impl OrderMachineEffects for TestEffects {
    fn calculate_total(&self, order: &Order) -> Money {
        Money {
            amount: order.quantity * 1000,
            currency: "USD".to_string(),
        }
    }

    fn process_payment(&self, total: &Money) -> Receipt {
        Receipt {
            transaction_id: format!("txn-{}", total.amount),
            amount: total.clone(),
        }
    }

    fn create_shipment(&self, order: &Order) -> String {
        format!("SHIP-{}-001", order.id.to_uppercase())
    }
}

impl PaymentMachineEffects for TestEffects {
    fn initiate_charge(&self, amount: &PayMoney) -> String {
        format!("charge-{}", amount.amount)
    }

    fn confirm_charge(&self, tx_id: &String, amount: &PayMoney) -> PayReceipt {
        PayReceipt {
            transaction_id: tx_id.clone(),
            amount: amount.clone(),
        }
    }
}

fn make_order(id: &str, items: &str, quantity: i64) -> Order {
    Order {
        id: id.to_string(),
        items: items.to_string(),
        quantity,
    }
}

fn make_pay_money(amount: i64) -> PayMoney {
    PayMoney {
        amount,
        currency: "USD".to_string(),
    }
}

// --- OrderMachine tests ---

#[test]
fn order_happy_path_full_lifecycle() {
    let fx = TestEffects;
    let mut machine = OrderMachine::new(make_order("ord-001", "widget", 3));

    assert!(matches!(machine.state(), OrderMachineState::Pending { .. }));

    // Pending -> Validated (3 units * 1000 = 3000 cents)
    machine.validate(&fx).expect("validate should succeed");
    assert!(matches!(machine.state(), OrderMachineState::Validated { .. }));

    if let OrderMachineState::Validated { total, .. } = machine.state() {
        assert_eq!(total.amount, 3000);
        assert_eq!(total.currency, "USD");
    } else {
        panic!("expected Validated state");
    }

    // Validated -> Charged
    machine.charge(&fx).expect("charge should succeed");
    assert!(matches!(machine.state(), OrderMachineState::Charged { .. }));

    if let OrderMachineState::Charged { receipt, .. } = machine.state() {
        assert_eq!(receipt.transaction_id, "txn-3000");
        assert_eq!(receipt.amount.amount, 3000);
    } else {
        panic!("expected Charged state");
    }

    // Charged -> Shipped
    machine.ship(&fx).expect("ship should succeed");
    assert!(matches!(machine.state(), OrderMachineState::Shipped { .. }));

    if let OrderMachineState::Shipped { order, tracking } = machine.state() {
        assert_eq!(order.id, "ord-001");
        assert_eq!(tracking, "SHIP-ORD-001-001");
    } else {
        panic!("expected Shipped state");
    }
}

#[test]
fn order_zero_quantity_routes_to_failed() {
    let fx = TestEffects;
    let mut machine = OrderMachine::new(make_order("ord-002", "nothing", 0));

    // validate call itself returns Ok, but machine routes to Failed.
    machine.validate(&fx).expect("validate call returns Ok");
    assert!(matches!(machine.state(), OrderMachineState::Failed { .. }));

    if let OrderMachineState::Failed { reason } = machine.state() {
        assert_eq!(reason, "invalid order total");
    } else {
        panic!("expected Failed state with specific reason");
    }
}

#[test]
fn order_explicit_fail_transition() {
    let mut machine = OrderMachine::new(make_order("ord-003", "items", 1));

    // Pending -> Failed via explicit fail transition.
    machine.fail().expect("fail from Pending should succeed");
    assert!(matches!(machine.state(), OrderMachineState::Failed { .. }));

    if let OrderMachineState::Failed { reason } = machine.state() {
        assert_eq!(reason, "payment declined");
    } else {
        panic!("expected Failed with 'payment declined'");
    }
}

#[test]
fn order_invalid_transition_from_pending_returns_error() {
    let fx = TestEffects;
    let mut machine = OrderMachine::new(make_order("ord-004", "items", 2));

    // charge requires Validated state; calling it from Pending must fail.
    let err = machine
        .charge(&fx)
        .expect_err("charge from Pending must return error");

    assert!(
        matches!(err, OrderMachineError::InvalidTransition { .. }),
        "expected InvalidTransition, got: {:?}",
        err
    );
}

#[test]
fn order_invalid_ship_from_validated_returns_error() {
    let fx = TestEffects;
    let mut machine = OrderMachine::new(make_order("ord-005", "items", 2));

    machine.validate(&fx).expect("validate should succeed");
    assert!(matches!(machine.state(), OrderMachineState::Validated { .. }));

    // ship requires Charged state; calling from Validated must fail.
    let err = machine
        .ship(&fx)
        .expect_err("ship from Validated must return error");

    assert!(
        matches!(err, OrderMachineError::InvalidTransition { .. }),
        "expected InvalidTransition, got: {:?}",
        err
    );
}

// --- PaymentMachine tests ---

#[test]
fn payment_happy_path() {
    let fx = TestEffects;
    let mut machine = PaymentMachine::new(make_pay_money(5000));

    assert!(matches!(machine.state(), PaymentMachineState::Awaiting { .. }));

    // Awaiting -> Processing
    machine.initiate(&fx).expect("initiate should succeed");
    assert!(matches!(machine.state(), PaymentMachineState::Processing { .. }));

    if let PaymentMachineState::Processing { tx_id, amount } = machine.state() {
        assert_eq!(tx_id, "charge-5000");
        assert_eq!(amount.amount, 5000);
    } else {
        panic!("expected Processing state");
    }

    // Processing -> Settled
    machine.confirm(&fx).expect("confirm should succeed");
    assert!(matches!(machine.state(), PaymentMachineState::Settled { .. }));

    if let PaymentMachineState::Settled { receipt } = machine.state() {
        assert_eq!(receipt.transaction_id, "charge-5000");
        assert_eq!(receipt.amount.amount, 5000);
    } else {
        panic!("expected Settled state");
    }
}

#[test]
fn payment_zero_amount_routes_to_declined() {
    let fx = TestEffects;
    let mut machine = PaymentMachine::new(make_pay_money(0));

    machine.initiate(&fx).expect("initiate call returns Ok");
    assert!(matches!(machine.state(), PaymentMachineState::Declined { .. }));

    if let PaymentMachineState::Declined { reason } = machine.state() {
        assert_eq!(reason, "amount must be positive");
    } else {
        panic!("expected Declined state");
    }
}

#[test]
fn payment_invalid_confirm_from_awaiting_returns_error() {
    let fx = TestEffects;
    let mut machine = PaymentMachine::new(make_pay_money(1000));

    // confirm requires Processing state; calling from Awaiting must fail.
    let err = machine
        .confirm(&fx)
        .expect_err("confirm from Awaiting must return error");

    assert!(
        matches!(err, PaymentMachineError::InvalidTransition { .. }),
        "expected InvalidTransition, got: {:?}",
        err
    );
}

// --- SupervisorMachine tests ---

#[test]
fn supervisor_failure_and_recovery() {
    let mut supervisor = SupervisorMachine::new();
    assert!(matches!(supervisor.state(), SupervisorMachineState::Running));

    // Running -> Degraded
    supervisor
        .report_failure(1)
        .expect("report_failure from Running should succeed");
    assert!(matches!(
        supervisor.state(),
        SupervisorMachineState::Degraded { failure_count: 1 }
    ));

    // Degraded -> Running
    supervisor.recover().expect("recover from Degraded should succeed");
    assert!(matches!(supervisor.state(), SupervisorMachineState::Running));
}

#[test]
fn supervisor_shutdown_after_persistent_failure() {
    let mut supervisor = SupervisorMachine::new();

    supervisor.report_failure(3).expect("report_failure should succeed");
    assert!(matches!(
        supervisor.state(),
        SupervisorMachineState::Degraded { .. }
    ));

    supervisor.shutdown().expect("shutdown from Degraded should succeed");
    assert!(matches!(supervisor.state(), SupervisorMachineState::Shutdown));
}

#[test]
fn supervisor_shutdown_is_terminal() {
    let mut supervisor = SupervisorMachine::new();

    supervisor.report_failure(1).unwrap();
    supervisor.shutdown().unwrap();
    assert!(matches!(supervisor.state(), SupervisorMachineState::Shutdown));

    // All transitions from Shutdown must fail.
    let err = supervisor
        .recover()
        .expect_err("recover from Shutdown must fail");
    assert!(
        matches!(err, SupervisorMachineError::InvalidTransition { .. }),
        "expected InvalidTransition, got: {:?}",
        err
    );
}

#[test]
fn supervisor_invalid_report_failure_from_degraded_returns_error() {
    let mut supervisor = SupervisorMachine::new();

    supervisor.report_failure(1).unwrap();
    assert!(matches!(
        supervisor.state(),
        SupervisorMachineState::Degraded { .. }
    ));

    // report_failure requires Running state; calling from Degraded must fail.
    let err = supervisor
        .report_failure(2)
        .expect_err("report_failure from Degraded must return error");
    assert!(
        matches!(err, SupervisorMachineError::InvalidTransition { .. }),
        "expected InvalidTransition, got: {:?}",
        err
    );
}

// --- Multi-machine coordination tests ---

#[test]
fn order_and_payment_machines_coordinate_for_happy_path() {
    let fx = TestEffects;

    // Create and validate an order.
    let mut order = OrderMachine::new(make_order("ord-coord-1", "items", 5));
    order.validate(&fx).expect("validate should succeed");

    // Extract the total and run it through the payment machine.
    let total = if let OrderMachineState::Validated { total, .. } = order.state() {
        total.clone()
    } else {
        panic!("expected Validated state");
    };

    assert_eq!(total.amount, 5000);

    let pay_amount = PayMoney {
        amount: total.amount,
        currency: total.currency.clone(),
    };
    let mut payment = PaymentMachine::new(pay_amount);
    payment.initiate(&fx).expect("payment initiate should succeed");
    payment.confirm(&fx).expect("payment confirm should succeed");
    assert!(matches!(payment.state(), PaymentMachineState::Settled { .. }));

    // With payment settled, complete the order.
    order.charge(&fx).expect("order charge should succeed");
    order.ship(&fx).expect("order ship should succeed");
    assert!(matches!(order.state(), OrderMachineState::Shipped { .. }));
}

#[test]
fn supervisor_tracks_order_failures() {
    let fx = TestEffects;
    let mut supervisor = SupervisorMachine::new();

    // Simulate a bad order failing.
    let mut order = OrderMachine::new(make_order("ord-fail-1", "x", 0));
    order.validate(&fx).unwrap();
    assert!(matches!(order.state(), OrderMachineState::Failed { .. }));

    // Supervisor records the failure.
    supervisor.report_failure(1).expect("should succeed from Running");
    assert!(matches!(
        supervisor.state(),
        SupervisorMachineState::Degraded { failure_count: 1 }
    ));

    // After operator intervention, supervisor recovers.
    supervisor.recover().expect("recover should succeed");
    assert!(matches!(supervisor.state(), SupervisorMachineState::Running));
}
