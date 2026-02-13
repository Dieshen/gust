# Microservice Example

```mermaid
flowchart LR
  API --> Order
  Order --> Payment
  Payment --> Supervisor
```

Run: `cargo run`
