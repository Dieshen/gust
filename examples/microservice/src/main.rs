// Each generated file is wrapped in its own module to avoid duplicate
// `use serde::{Serialize, Deserialize}` imports when multiple machines
// are included in the same crate.
pub mod order_machine {
    include!("order.g.rs");
}

pub mod payment_machine {
    include!("payment.g.rs");
}

pub mod supervisor_machine {
    include!("supervisor.g.rs");
}

// Re-export everything so the rest of the crate can use unqualified names.
pub use order_machine::*;
pub use payment_machine::*;
pub use supervisor_machine::*;

mod effects;
use effects::MicroserviceEffects;

fn main() {
    let fx = MicroserviceEffects;

    println!("=== Microservice Example: Multi-Machine Coordination ===\n");

    // --- Scenario 1: Full happy-path order lifecycle ---
    //
    // An order is created, validated, charged via the payment machine,
    // and then shipped. The supervisor stays healthy throughout.
    println!("-- Scenario 1: Full order lifecycle (happy path) --");
    run_happy_path(&fx);

    println!();

    // --- Scenario 2: Invalid order causes failed validation ---
    //
    // An order with zero quantity produces a zero-amount total.
    // The order machine validates to Failed, and the supervisor records the failure.
    println!("-- Scenario 2: Invalid order -> validation failure --");
    run_decline_path(&fx);

    println!();

    // --- Scenario 3: Supervisor failure tracking and recovery ---
    println!("-- Scenario 3: Supervisor tracks failures and recovers --");
    run_supervisor_demo();

    println!("\nAll scenarios completed successfully.");
}

// run_happy_path drives a valid order through validate -> charge -> ship.
// Payment is processed via an independent PaymentMachine to demonstrate
// multi-machine coordination: the order machine delegates charging to
// the payment machine, then uses the result to advance its own state.
fn run_happy_path(fx: &MicroserviceEffects) {
    let supervisor = SupervisorMachine::new();
    assert!(matches!(supervisor.state(), SupervisorMachineState::Running));

    let order = Order {
        id: "ord-001".to_string(),
        items: "widget,gadget".to_string(),
        quantity: 3,
    };

    let mut order_machine = OrderMachine::new(order);
    println!("  Order initial state:   {:?}", order_machine.state());

    // Validate: calculate total (3 * $10 = $30 = 3000 cents).
    order_machine.validate(fx).expect("validate should succeed");
    println!("  After validate:        {:?}", order_machine.state());

    // Coordinate with payment machine: extract total and run it through PaymentMachine.
    let total = if let OrderMachineState::Validated { total, .. } = order_machine.state() {
        total.clone()
    } else {
        panic!("expected Validated state");
    };

    let pay_amount = PayMoney {
        amount: total.amount,
        currency: total.currency.clone(),
    };
    let mut payment_machine = PaymentMachine::new(pay_amount);
    println!("  Payment initial state: {:?}", payment_machine.state());

    payment_machine.initiate(fx).expect("initiate charge should succeed");
    println!("  After initiate:        {:?}", payment_machine.state());

    payment_machine.confirm(fx).expect("confirm charge should succeed");
    println!("  After confirm:         {:?}", payment_machine.state());

    let pay_settled = matches!(payment_machine.state(), PaymentMachineState::Settled { .. });
    assert!(pay_settled, "payment must be Settled before charging order");

    // Charge the order machine (uses the order effects to simulate the receipt).
    order_machine.charge(fx).expect("charge should succeed");
    println!("  After order charge:    {:?}", order_machine.state());

    // Ship the order.
    order_machine.ship(fx).expect("ship should succeed");
    println!("  After ship:            {:?}", order_machine.state());

    // Extract and display the final tracking number.
    if let OrderMachineState::Shipped { order, tracking } = order_machine.state() {
        println!("  Order {} shipped with tracking: {}", order.id, tracking);
    } else {
        panic!("expected Shipped state");
    }

    // Supervisor remains healthy — no failures occurred.
    assert!(matches!(supervisor.state(), SupervisorMachineState::Running));
    println!("  Supervisor state:      {:?} (no failures)", supervisor.state());
}

// run_decline_path demonstrates an order with zero quantity failing validation,
// and the supervisor recording the failure.
fn run_decline_path(fx: &MicroserviceEffects) {
    let mut supervisor = SupervisorMachine::new();

    // Zero quantity -> total = 0 cents -> validation fails.
    let bad_order = Order {
        id: "ord-002".to_string(),
        items: "nothing".to_string(),
        quantity: 0,
    };

    let mut order_machine = OrderMachine::new(bad_order);
    order_machine.validate(fx).expect("validate call itself returns Ok");
    println!("  After validate (qty=0): {:?}", order_machine.state());

    assert!(
        matches!(order_machine.state(), OrderMachineState::Failed { .. }),
        "expected Failed state for zero-quantity order"
    );

    if let OrderMachineState::Failed { reason } = order_machine.state() {
        println!("  Failure reason: {}", reason);
    }

    // Supervisor records the failure — transitions to Degraded.
    supervisor
        .report_failure(1)
        .expect("report_failure from Running should succeed");
    println!("  Supervisor after failure: {:?}", supervisor.state());

    assert!(matches!(
        supervisor.state(),
        SupervisorMachineState::Degraded { failure_count: 1 }
    ));

    // Also demonstrate PaymentMachine declining a zero-amount charge.
    let zero_amount = PayMoney {
        amount: 0,
        currency: "USD".to_string(),
    };
    let mut payment_machine = PaymentMachine::new(zero_amount);
    payment_machine.initiate(fx).expect("initiate call returns Ok");
    println!("  Payment machine (amt=0): {:?}", payment_machine.state());
    assert!(matches!(
        payment_machine.state(),
        PaymentMachineState::Declined { .. }
    ));
}

// run_supervisor_demo shows failure accumulation, recovery, and eventual shutdown.
fn run_supervisor_demo() {
    let mut supervisor = SupervisorMachine::new();
    println!("  Initial:               {:?}", supervisor.state());

    // First failure: Running -> Degraded.
    supervisor
        .report_failure(1)
        .expect("report_failure from Running should succeed");
    println!("  After 1st failure:     {:?}", supervisor.state());
    assert!(matches!(
        supervisor.state(),
        SupervisorMachineState::Degraded { .. }
    ));

    // Recovery: Degraded -> Running.
    supervisor.recover().expect("recover from Degraded should succeed");
    println!("  After recovery:        {:?}", supervisor.state());
    assert!(matches!(supervisor.state(), SupervisorMachineState::Running));

    // Second failure: Running -> Degraded again.
    supervisor
        .report_failure(2)
        .expect("report_failure from Running should succeed");
    println!("  After 2nd failure:     {:?}", supervisor.state());

    // Persistent failure triggers shutdown: Degraded -> Shutdown.
    supervisor.shutdown().expect("shutdown from Degraded should succeed");
    println!("  After shutdown:        {:?}", supervisor.state());
    assert!(matches!(supervisor.state(), SupervisorMachineState::Shutdown));

    // Verify shutdown is terminal: further transitions must fail.
    let err = supervisor
        .recover()
        .expect_err("recover from Shutdown must fail");
    println!("  Invalid recover from Shutdown: {}", err);
}
