// MicroserviceEffects provides deterministic implementations of all three machine
// effect traits. No external I/O — all logic is in-process for testability.
//
// Types are defined in the generated code included via the modules in main.rs
// and re-exported into the crate root with `pub use`.

pub struct MicroserviceEffects;

// OrderMachineEffects: simulates order processing business logic.
impl crate::OrderMachineEffects for MicroserviceEffects {
    fn calculate_total(&self, order: &crate::Order) -> crate::Money {
        // Price is quantity * 1000 cents (i.e., $10 per unit in the order currency).
        crate::Money {
            amount: order.quantity * 1000,
            currency: "USD".to_string(),
        }
    }

    fn process_payment(&self, total: &crate::Money) -> crate::Receipt {
        // Generate a deterministic transaction ID from the amount.
        crate::Receipt {
            transaction_id: format!("txn-{}", total.amount),
            amount: total.clone(),
        }
    }

    fn create_shipment(&self, order: &crate::Order) -> String {
        // Return a deterministic tracking number derived from the order ID.
        format!("SHIP-{}-001", order.id.to_uppercase())
    }
}

// PaymentMachineEffects: simulates payment gateway interactions.
impl crate::PaymentMachineEffects for MicroserviceEffects {
    fn initiate_charge(&self, amount: &crate::PayMoney) -> String {
        // Return a deterministic transaction ID for the charge initiation.
        format!("charge-{}", amount.amount)
    }

    fn confirm_charge(&self, tx_id: &str, amount: &crate::PayMoney) -> crate::PayReceipt {
        crate::PayReceipt {
            transaction_id: tx_id.to_string(),
            amount: amount.clone(),
        }
    }
}
