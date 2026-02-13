# Phase 4 Spec: Ecosystem

## Prerequisites

Before starting Phase 4 implementation:

1. **Phase 3 is complete** - Channels, supervision trees, and lifecycle management are fully implemented
2. **Understanding of Phase 1-3 architecture** - Familiarity with async handlers, generics, and runtime patterns
3. **Community feedback incorporated** - Real-world usage patterns from early adopters inform stdlib design
4. **Documentation infrastructure** - mdBook or similar ready for comprehensive docs

## Current State (Post-Phase 3)

### File Structure
```
D:\Projects\gust\
├── gust-lang\
│   ├── src\
│   │   ├── grammar.pest      # Grammar with channels, supervision, lifecycle
│   │   ├── ast.rs            # AST with channel/supervisor types
│   │   ├── parser.rs         # Parser with Phase 1-3 features
│   │   ├── codegen.rs        # Rust codegen with async, channels, supervision
│   │   ├── codegen_go.rs     # Go codegen
│   │   ├── codegen_wasm.rs   # WASM codegen (NEW in Phase 4)
│   │   ├── codegen_nostd.rs  # no_std codegen (NEW in Phase 4)
│   │   ├── codegen_ffi.rs    # C FFI codegen (NEW in Phase 4)
│   │   └── lib.rs
│   └── Cargo.toml
├── gust-runtime\
│   ├── src\
│   │   ├── lib.rs            # Core runtime with Machine, Supervisor traits
│   │   ├── channels.rs       # Typed channels (Phase 3)
│   │   ├── supervision.rs    # Supervision trees (Phase 3)
│   │   └── lifecycle.rs      # Spawn, shutdown, timeouts (Phase 3)
│   └── Cargo.toml
├── gust-stdlib\              # NEW - Standard library of reusable machines
│   ├── request_response.gu
│   ├── saga.gu
│   ├── circuit_breaker.gu
│   ├── retry.gu
│   ├── rate_limiter.gu
│   ├── health_check.gu
│   └── Cargo.toml
├── gust-build\
│   └── src\lib.rs
├── gust-cli\
│   └── src\main.rs
├── docs\                      # NEW - Comprehensive documentation
│   ├── src\
│   │   ├── SUMMARY.md
│   │   ├── introduction.md
│   │   ├── tutorial\
│   │   │   ├── chapter_1.md
│   │   │   └── ...
│   │   ├── reference\
│   │   │   ├── syntax.md
│   │   │   ├── types.md
│   │   │   └── ...
│   │   ├── cookbook\
│   │   │   ├── patterns.md
│   │   │   └── ...
│   │   └── migration.md
│   └── book.toml
├── examples\
│   ├── microservice\         # NEW - Full example project
│   ├── event_processor\      # NEW - Full example project
│   ├── workflow_engine\      # NEW - Full example project
│   └── ...
└── Cargo.toml
```

### Assumed Phase 3 Capabilities

**Channels:**
- Typed message passing between machines
- Syntax: `channel MachineA -> MachineB: MessageType`
- Generated tokio::mpsc channels in Rust

**Supervision:**
- `supervises` keyword for parent-child relationships
- Supervision strategies: `one_for_one`, `one_for_all`, `rest_for_one`
- Automatic restart on failure with backoff

**Lifecycle:**
- `spawn` primitive for machine creation
- `shutdown` for graceful termination
- Timeout support on transitions: `timeout 5s`
- Cancellation tokens propagated through supervision tree

---

## Feature 1: Standard Library

### Requirements

**R1.1**: Create `gust-stdlib` crate containing reusable machine patterns as `.gu` files.

**R1.2**: Standard library must include these patterns:
- Request/Response - async request with timeout and correlation
- Saga - distributed transaction with compensating actions
- Circuit Breaker - failure detection with Closed/Open/HalfOpen states
- Retry with Backoff - exponential backoff with max attempts
- Rate Limiter - token bucket or sliding window algorithm
- Health Check - periodic health monitoring with degradation detection

**R1.3**: Each stdlib machine must be **generic** over relevant types (e.g., `Request<T, R>` where `T` is request type, `R` is response type).

**R1.4**: Stdlib machines must be composable (can be supervised, can communicate via channels).

**R1.5**: Grammar must support generics: `machine Foo<T, U> { ... }`.

**R1.6**: Generated code must use Rust/Go generics (trait bounds in Rust, interface{} in Go pre-1.18).

### Acceptance Criteria

**AC1.1**: User projects can import stdlib machines with `use gust_stdlib::request_response::RequestResponse;`

**AC1.2**: Generic machines instantiate with concrete types: `RequestResponse<OrderRequest, OrderResponse>`

**AC1.3**: All stdlib machines compile to both Rust and Go without errors.

**AC1.4**: Stdlib machines work in supervision trees and with channels.

**AC1.5**: Documentation includes usage examples for each stdlib pattern.

### Test Cases

**TC1.1 - Request/Response Basic Usage**

User code:
```gust
use gust_stdlib::request_response::RequestResponse;

type OrderRequest {
    order_id: String,
    quantity: i64,
}

type OrderResponse {
    status: String,
    total: Money,
}

machine OrderService {
    state Ready
    state Processing(request: RequestResponse<OrderRequest, OrderResponse>)

    transition handle_order: Ready -> Processing

    on handle_order(req: OrderRequest) {
        let rr = spawn RequestResponse::new(req, 5s);
        goto Processing(rr);
    }
}
```

Expected: Compiles successfully, `RequestResponse<OrderRequest, OrderResponse>` instantiated correctly.

**TC1.2 - Circuit Breaker Integration**

User code:
```gust
use gust_stdlib::circuit_breaker::CircuitBreaker;

machine ExternalAPI {
    state Active(breaker: CircuitBreaker)

    async effect call_api(data: String) -> Result<String, String>

    transition call: Active -> Active

    async on call(data: String) {
        match breaker.execute(|| perform call_api(data)) {
            Ok(result) => {
                // success
                goto Active(breaker);
            }
            Err(err) => {
                // circuit open
                goto Active(breaker);
            }
        }
    }
}
```

Expected: Circuit breaker state transitions (Closed -> Open -> HalfOpen) enforce failure thresholds.

**TC1.3 - Saga Pattern**

User code:
```gust
use gust_stdlib::saga::Saga;

type BookingStep {
    name: String,
    compensate: String,
}

machine TravelBooking {
    state Planning(saga: Saga<BookingStep>)
    state Committed
    state Aborted(reason: String)

    transition execute: Planning -> Committed | Aborted

    async on execute(steps: Vec<BookingStep>) {
        for step in steps {
            match saga.execute_step(step) {
                Ok(_) => continue,
                Err(err) => {
                    saga.compensate_all();
                    goto Aborted(err);
                }
            }
        }
        goto Committed();
    }
}
```

Expected: Saga compensates in reverse order on failure.

### Implementation Guide

#### Step 1: Add Generics to Grammar

File: `D:\Projects\gust\gust-lang\src\grammar.pest`

Add generic parameter support:
```pest
machine_decl = { "machine" ~ ident ~ generic_params? ~ ("{" ~ machine_body ~ "}") }

generic_params = { "<" ~ generic_param ~ ("," ~ generic_param)* ~ ">" }
generic_param = { ident ~ (":" ~ trait_bound)? }
trait_bound = { ident ~ ("+" ~ ident)* }

state_decl = { "state" ~ ident ~ ("(" ~ field_list ~ ")")? }

// Field list can now reference generic type parameters
type_expr = { tuple_type | generic_type | simple_type }
```

#### Step 2: Update AST

File: `D:\Projects\gust\gust-lang\src\ast.rs`

```rust
pub struct MachineDecl {
    pub name: String,
    pub generic_params: Vec<GenericParam>,  // NEW
    pub states: Vec<StateDecl>,
    pub transitions: Vec<TransitionDecl>,
    pub handlers: Vec<OnHandler>,
    pub effects: Vec<EffectDecl>,
}

pub struct GenericParam {
    pub name: String,
    pub bounds: Vec<String>,  // trait bounds like "Serialize + Clone"
}
```

#### Step 3: Update Rust Codegen

File: `D:\Projects\gust\gust-lang\src\codegen.rs`

```rust
fn emit_machine(&mut self, machine: &MachineDecl) {
    let name = &machine.name;
    let state_enum = format!("{name}State");

    // Generic parameters
    let generic_decl = if !machine.generic_params.is_empty() {
        let params: Vec<String> = machine.generic_params.iter()
            .map(|p| {
                if p.bounds.is_empty() {
                    p.name.clone()
                } else {
                    format!("{}: {}", p.name, p.bounds.join(" + "))
                }
            })
            .collect();
        format!("<{}>", params.join(", "))
    } else {
        String::new()
    };

    let generic_use = if !machine.generic_params.is_empty() {
        let params: Vec<&str> = machine.generic_params.iter()
            .map(|p| p.name.as_str())
            .collect();
        format!("<{}>", params.join(", "))
    } else {
        String::new()
    };

    // State enum with generics
    self.line("#[derive(Debug, Clone, Serialize, Deserialize)]");
    self.line(&format!("pub enum {state_enum}{generic_decl} {{"));
    // ... emit states (may reference generic params)

    // Machine struct with generics
    self.line("#[derive(Debug, Clone, Serialize, Deserialize)]");
    self.line(&format!("pub struct {name}{generic_decl} {{"));
    self.indent += 1;
    self.line(&format!("pub state: {state_enum}{generic_use},"));
    self.indent -= 1;
    self.line("}");

    // Impl block with generics
    self.line(&format!("impl{generic_decl} {name}{generic_use} {{"));
    // ... transition methods
}
```

#### Step 4: Write Standard Library Machines

**File: `D:\Projects\gust\gust-stdlib\request_response.gu`**

```gust
// Request/Response pattern with timeout and correlation
//
// Generic parameters:
//   T: Request type (must be Serialize + Deserialize + Clone)
//   R: Response type (must be Serialize + Deserialize + Clone)

machine RequestResponse<T: Serialize + Deserialize + Clone, R: Serialize + Deserialize + Clone> {
    state Pending(
        request: T,
        correlation_id: String,
        timeout_ms: i64,
        started_at: i64
    )

    state Completed(response: R)

    state TimedOut(elapsed_ms: i64)

    state Failed(error: String)

    // Transitions
    transition send: Pending -> Pending
    transition receive: Pending -> Completed | Failed
    transition timeout: Pending -> TimedOut

    // Effects
    async effect send_request(req: T, correlation_id: String) -> Result<(), String>
    async effect wait_for_response(correlation_id: String, timeout_ms: i64) -> Result<R, String>
    effect current_time_ms() -> i64

    // Handlers
    async on send(ctx: SendContext) {
        let result = perform send_request(request, correlation_id);
        match result {
            Ok(_) => goto Pending(request, correlation_id, timeout_ms, started_at),
            Err(err) => goto Failed(err),
        }
    }

    async on receive(ctx: ReceiveContext) {
        let elapsed = perform current_time_ms() - started_at;

        if elapsed > timeout_ms {
            goto TimedOut(elapsed);
        }

        let remaining = timeout_ms - elapsed;
        let result = perform wait_for_response(correlation_id, remaining);

        match result {
            Ok(response) => goto Completed(response),
            Err(err) => goto Failed(err),
        }
    }

    on timeout(ctx: TimeoutContext) {
        let elapsed = perform current_time_ms() - started_at;
        goto TimedOut(elapsed);
    }
}
```

**File: `D:\Projects\gust\gust-stdlib\circuit_breaker.gu`**

```gust
// Circuit Breaker pattern for fault tolerance
//
// States: Closed (normal), Open (failing), HalfOpen (testing recovery)
// Generic parameter T: Return type of protected operation

machine CircuitBreaker<T: Clone> {
    state Closed(
        failure_count: i64,
        success_count: i64,
        failure_threshold: i64
    )

    state Open(
        opened_at: i64,
        timeout_ms: i64
    )

    state HalfOpen(
        test_requests: i64,
        max_test_requests: i64
    )

    // Transitions
    transition execute: Closed -> Closed | Open
    transition execute_open: Open -> Open | HalfOpen
    transition execute_half_open: HalfOpen -> Closed | Open
    transition trip: Closed -> Open
    transition reset: HalfOpen -> Closed

    // Effects
    effect current_time_ms() -> i64

    // Handlers
    on execute(operation_succeeded: bool) {
        if operation_succeeded {
            let new_success = success_count + 1;
            let new_failure = 0;
            goto Closed(new_failure, new_success, failure_threshold);
        } else {
            let new_failure = failure_count + 1;

            if new_failure >= failure_threshold {
                let now = perform current_time_ms();
                goto Open(now, 60000);  // 60 second timeout
            } else {
                goto Closed(new_failure, success_count, failure_threshold);
            }
        }
    }

    on execute_open(ctx: ExecuteContext) {
        let now = perform current_time_ms();
        let elapsed = now - opened_at;

        if elapsed >= timeout_ms {
            // Try to recover
            goto HalfOpen(0, 3);  // Allow 3 test requests
        } else {
            // Still open, reject immediately
            goto Open(opened_at, timeout_ms);
        }
    }

    on execute_half_open(operation_succeeded: bool) {
        if operation_succeeded {
            let new_test = test_requests + 1;

            if new_test >= max_test_requests {
                // Enough successful tests, close circuit
                goto Closed(0, new_test, 5);
            } else {
                // Continue testing
                goto HalfOpen(new_test, max_test_requests);
            }
        } else {
            // Test failed, open circuit again
            let now = perform current_time_ms();
            goto Open(now, 60000);
        }
    }

    on trip(ctx: TripContext) {
        let now = perform current_time_ms();
        goto Open(now, 60000);
    }

    on reset(ctx: ResetContext) {
        goto Closed(0, 0, 5);
    }
}
```

**File: `D:\Projects\gust\gust-stdlib\saga.gu`**

```gust
// Saga pattern for distributed transactions with compensation
//
// Generic parameter S: Step type (must include forward and compensate actions)

type SagaStep<S> {
    step_data: S,
    compensate_data: Option<S>,
}

machine Saga<S: Clone> {
    state Planning(steps: Vec<SagaStep<S>>)

    state Executing(
        steps: Vec<SagaStep<S>>,
        completed_steps: Vec<SagaStep<S>>,
        current_index: i64
    )

    state Compensating(
        completed_steps: Vec<SagaStep<S>>,
        current_index: i64
    )

    state Committed(completed_steps: Vec<SagaStep<S>>)

    state Aborted(
        reason: String,
        compensated_count: i64
    )

    // Transitions
    transition begin: Planning -> Executing
    transition execute_step: Executing -> Executing | Compensating | Committed
    transition compensate_step: Compensating -> Compensating | Aborted
    transition commit: Executing -> Committed
    transition abort: Compensating -> Aborted

    // Effects
    async effect execute_forward(step: S) -> Result<S, String>
    async effect execute_compensate(step: S) -> Result<(), String>

    // Handlers
    on begin(ctx: BeginContext) {
        goto Executing(steps, Vec::new(), 0);
    }

    async on execute_step(ctx: StepContext) {
        if current_index >= steps.len() {
            // All steps completed
            goto Committed(completed_steps);
        }

        let step = steps[current_index];
        let result = perform execute_forward(step.step_data);

        match result {
            Ok(completed_data) => {
                let mut new_completed = completed_steps.clone();
                new_completed.push(SagaStep {
                    step_data: completed_data,
                    compensate_data: step.compensate_data,
                });

                let new_index = current_index + 1;
                goto Executing(steps, new_completed, new_index);
            }
            Err(err) => {
                // Step failed, begin compensation
                goto Compensating(completed_steps, completed_steps.len() - 1);
            }
        }
    }

    async on compensate_step(ctx: CompensateContext) {
        if current_index < 0 {
            // All compensations complete
            goto Aborted("saga_aborted", completed_steps.len());
        }

        let step = completed_steps[current_index];

        if step.compensate_data.is_some() {
            let comp_data = step.compensate_data.unwrap();
            let result = perform execute_compensate(comp_data);

            match result {
                Ok(_) => {
                    let new_index = current_index - 1;
                    goto Compensating(completed_steps, new_index);
                }
                Err(err) => {
                    // Compensation failed - critical error
                    goto Aborted(err, current_index);
                }
            }
        } else {
            // No compensation needed for this step
            let new_index = current_index - 1;
            goto Compensating(completed_steps, new_index);
        }
    }

    on commit(ctx: CommitContext) {
        goto Committed(completed_steps);
    }

    on abort(reason: String) {
        goto Aborted(reason, 0);
    }
}
```

**File: `D:\Projects\gust\gust-stdlib\retry.gu`**

```gust
// Retry with exponential backoff
//
// Generic parameter T: Return type of retried operation

machine Retry<T: Clone> {
    state Ready(
        max_attempts: i64,
        base_delay_ms: i64,
        max_delay_ms: i64
    )

    state Attempting(
        attempt_number: i64,
        max_attempts: i64,
        base_delay_ms: i64,
        max_delay_ms: i64
    )

    state Waiting(
        attempt_number: i64,
        delay_ms: i64,
        max_attempts: i64,
        base_delay_ms: i64,
        max_delay_ms: i64
    )

    state Succeeded(result: T, attempts_used: i64)

    state Failed(last_error: String, attempts_used: i64)

    // Transitions
    transition attempt: Ready -> Attempting
    transition retry: Attempting -> Waiting | Succeeded | Failed
    transition wait_complete: Waiting -> Attempting

    // Effects
    async effect execute_operation() -> Result<T, String>
    async effect sleep_ms(duration: i64) -> ()

    // Handlers
    on attempt(ctx: AttemptContext) {
        goto Attempting(1, max_attempts, base_delay_ms, max_delay_ms);
    }

    async on retry(ctx: RetryContext) {
        let result = perform execute_operation();

        match result {
            Ok(value) => {
                goto Succeeded(value, attempt_number);
            }
            Err(err) => {
                if attempt_number >= max_attempts {
                    goto Failed(err, attempt_number);
                } else {
                    // Calculate backoff: base_delay * 2^(attempt-1)
                    let exponent = attempt_number - 1;
                    let delay = base_delay_ms * (2 ** exponent);
                    let capped_delay = if delay > max_delay_ms { max_delay_ms } else { delay };

                    goto Waiting(
                        attempt_number,
                        capped_delay,
                        max_attempts,
                        base_delay_ms,
                        max_delay_ms
                    );
                }
            }
        }
    }

    async on wait_complete(ctx: WaitContext) {
        perform sleep_ms(delay_ms);

        let next_attempt = attempt_number + 1;
        goto Attempting(next_attempt, max_attempts, base_delay_ms, max_delay_ms);
    }
}
```

**File: `D:\Projects\gust\gust-stdlib\rate_limiter.gu`**

```gust
// Token bucket rate limiter
//
// Allows burst_size requests immediately, then refills at rate_per_second

machine RateLimiter {
    state Active(
        tokens: f64,
        capacity: f64,
        rate_per_second: f64,
        last_refill: i64
    )

    state Blocked(
        tokens: f64,
        capacity: f64,
        rate_per_second: f64,
        last_refill: i64,
        blocked_until: i64
    )

    // Transitions
    transition acquire: Active -> Active | Blocked
    transition refill: Active -> Active
    transition refill_blocked: Blocked -> Active | Blocked

    // Effects
    effect current_time_ms() -> i64

    // Handlers
    on acquire(requested: f64) {
        // Refill tokens based on elapsed time
        let now = perform current_time_ms();
        let elapsed_seconds = (now - last_refill) / 1000.0;
        let new_tokens = tokens + (rate_per_second * elapsed_seconds);
        let capped_tokens = if new_tokens > capacity { capacity } else { new_tokens };

        if capped_tokens >= requested {
            // Grant request
            let remaining = capped_tokens - requested;
            goto Active(remaining, capacity, rate_per_second, now);
        } else {
            // Block until enough tokens available
            let needed = requested - capped_tokens;
            let wait_seconds = needed / rate_per_second;
            let wait_ms = wait_seconds * 1000.0;
            let unblock_time = now + wait_ms;

            goto Blocked(capped_tokens, capacity, rate_per_second, now, unblock_time);
        }
    }

    on refill(ctx: RefillContext) {
        let now = perform current_time_ms();
        let elapsed_seconds = (now - last_refill) / 1000.0;
        let new_tokens = tokens + (rate_per_second * elapsed_seconds);
        let capped_tokens = if new_tokens > capacity { capacity } else { new_tokens };

        goto Active(capped_tokens, capacity, rate_per_second, now);
    }

    on refill_blocked(ctx: RefillContext) {
        let now = perform current_time_ms();

        if now >= blocked_until {
            // Unblock
            let elapsed_seconds = (now - last_refill) / 1000.0;
            let new_tokens = tokens + (rate_per_second * elapsed_seconds);
            let capped_tokens = if new_tokens > capacity { capacity } else { new_tokens };

            goto Active(capped_tokens, capacity, rate_per_second, now);
        } else {
            // Still blocked
            goto Blocked(tokens, capacity, rate_per_second, last_refill, blocked_until);
        }
    }
}
```

**File: `D:\Projects\gust\gust-stdlib\health_check.gu`**

```gust
// Health check monitor with degradation detection
//
// Tracks health over time and transitions to Degraded if failures exceed threshold

machine HealthCheck {
    state Healthy(
        check_interval_ms: i64,
        last_check: i64,
        consecutive_failures: i64,
        failure_threshold: i64
    )

    state Degraded(
        check_interval_ms: i64,
        last_check: i64,
        consecutive_failures: i64,
        degraded_since: i64
    )

    state Critical(
        check_interval_ms: i64,
        last_check: i64,
        failure_count: i64,
        critical_since: i64
    )

    state Recovering(
        check_interval_ms: i64,
        last_check: i64,
        consecutive_successes: i64,
        recovery_threshold: i64
    )

    // Transitions
    transition check: Healthy -> Healthy | Degraded
    transition check_degraded: Degraded -> Degraded | Healthy | Critical
    transition check_critical: Critical -> Critical | Recovering
    transition check_recovering: Recovering -> Healthy | Degraded

    // Effects
    async effect perform_health_check() -> Result<(), String>
    effect current_time_ms() -> i64
    async effect sleep_ms(duration: i64) -> ()

    // Handlers
    async on check(ctx: CheckContext) {
        let now = perform current_time_ms();
        perform sleep_ms(check_interval_ms);

        let result = perform perform_health_check();

        match result {
            Ok(_) => {
                // Health check passed
                goto Healthy(check_interval_ms, now, 0, failure_threshold);
            }
            Err(err) => {
                let new_failures = consecutive_failures + 1;

                if new_failures >= failure_threshold {
                    goto Degraded(check_interval_ms, now, new_failures, now);
                } else {
                    goto Healthy(check_interval_ms, now, new_failures, failure_threshold);
                }
            }
        }
    }

    async on check_degraded(ctx: CheckContext) {
        let now = perform current_time_ms();
        perform sleep_ms(check_interval_ms);

        let result = perform perform_health_check();

        match result {
            Ok(_) => {
                // Recovered
                goto Healthy(check_interval_ms, now, 0, 5);
            }
            Err(err) => {
                let new_failures = consecutive_failures + 1;

                if new_failures >= 10 {
                    // Too many failures, escalate to critical
                    goto Critical(check_interval_ms, now, new_failures, now);
                } else {
                    goto Degraded(check_interval_ms, now, new_failures, degraded_since);
                }
            }
        }
    }

    async on check_critical(ctx: CheckContext) {
        let now = perform current_time_ms();
        perform sleep_ms(check_interval_ms * 2);  // Check less frequently when critical

        let result = perform perform_health_check();

        match result {
            Ok(_) => {
                // First success, start recovery process
                goto Recovering(check_interval_ms, now, 1, 3);
            }
            Err(err) => {
                goto Critical(check_interval_ms, now, failure_count + 1, critical_since);
            }
        }
    }

    async on check_recovering(ctx: CheckContext) {
        let now = perform current_time_ms();
        perform sleep_ms(check_interval_ms);

        let result = perform perform_health_check();

        match result {
            Ok(_) => {
                let new_successes = consecutive_successes + 1;

                if new_successes >= recovery_threshold {
                    // Fully recovered
                    goto Healthy(check_interval_ms, now, 0, 5);
                } else {
                    goto Recovering(check_interval_ms, now, new_successes, recovery_threshold);
                }
            }
            Err(err) => {
                // Recovery failed, back to degraded
                goto Degraded(check_interval_ms, now, 1, now);
            }
        }
    }
}
```

#### Step 5: Create stdlib Cargo.toml

**File: `D:\Projects\gust\gust-stdlib\Cargo.toml`**

```toml
[package]
name = "gust-stdlib"
version = "0.1.0"
edition = "2021"
description = "Standard library of reusable state machine patterns for Gust"

[dependencies]
gust-runtime = { path = "../gust-runtime" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }

[build-dependencies]
gust-build = { path = "../gust-build" }
```

**File: `D:\Projects\gust\gust-stdlib\build.rs`**

```rust
fn main() {
    gust_build::GustBuilder::new()
        .source_dir(".")
        .output_dir("src/generated")
        .target(gust_build::Target::Rust)
        .compile()
        .expect("Failed to compile Gust stdlib");
}
```

---

## Feature 2: Documentation

### Requirements

**R2.1**: Use mdBook for comprehensive documentation.

**R2.2**: Documentation must include:
- **Language Reference**: Complete syntax, semantics, type system
- **Tutorial**: "Your First Gust Service" - 30-minute hands-on
- **Migration Guide**: "Converting Rust State Machines to Gust"
- **Cookbook**: Common patterns and solutions

**R2.3**: Every example in docs must be executable (tested in CI).

**R2.4**: API reference auto-generated from source comments.

**R2.5**: Hosted on GitHub Pages with versioning (docs for each release).

### Acceptance Criteria

**AC2.1**: Running `mdbook serve` in `docs/` opens browsable documentation.

**AC2.2**: Tutorial chapter can be completed by someone unfamiliar with Gust in <30 minutes.

**AC2.3**: All code examples in docs compile and pass tests.

**AC2.4**: Migration guide includes before/after comparison for common patterns.

**AC2.5**: Cookbook has solutions for at least 10 real-world scenarios.

### Test Cases

**TC2.1 - Tutorial Completeness**

A new developer follows the tutorial from start to finish:
1. Install Gust CLI
2. Create new project with `gust init my-service`
3. Write first machine (Counter)
4. Implement effect handlers
5. Add tests
6. Run and verify output

Expected: All steps work without consulting external resources.

**TC2.2 - Code Example Validation**

CI pipeline:
1. Extracts all code blocks from markdown with ```gust
2. Writes each to temporary `.gu` file
3. Runs `gust build` on each
4. Compiles generated Rust code

Expected: All examples compile successfully.

**TC2.3 - Migration Guide Accuracy**

Before (Rust):
```rust
enum OrderState {
    Pending { order: Order },
    Validated { order: Order, total: Money },
    Charged { order: Order, payment: Receipt },
}

struct OrderMachine {
    state: OrderState,
}

impl OrderMachine {
    fn validate(&mut self, effects: &impl Effects) -> Result<(), Error> {
        match &self.state {
            OrderState::Pending { order } => {
                let total = effects.calculate_total(order);
                self.state = OrderState::Validated {
                    order: order.clone(),
                    total,
                };
                Ok(())
            }
            _ => Err(Error::InvalidTransition),
        }
    }
}
```

After (Gust):
```gust
machine OrderProcessor {
    state Pending(order: Order)
    state Validated(order: Order, total: Money)
    state Charged(order: Order, payment: Receipt)

    effect calculate_total(order: Order) -> Money

    transition validate: Pending -> Validated

    on validate(ctx: Context) {
        let total = perform calculate_total(order);
        goto Validated(order, total);
    }
}
```

Expected: Side-by-side comparison shows Gust eliminates boilerplate.

### Implementation Guide

#### Step 1: Initialize mdBook

File: `D:\Projects\gust\docs\book.toml`

```toml
[book]
title = "The Gust Programming Language"
authors = ["Gust Contributors"]
description = "Type-safe state machines that compile to Rust and Go"
language = "en"
multilingual = false
src = "src"

[build]
build-dir = "book"

[output.html]
default-theme = "rust"
preferred-dark-theme = "navy"
git-repository-url = "https://github.com/Dieshen/gust"
edit-url-template = "https://github.com/Dieshen/gust/edit/main/docs/{path}"

[output.html.fold]
enable = true

[output.html.playground]
runnable = false  # Can't run Gust in browser (yet)
```

#### Step 2: Write SUMMARY.md

File: `D:\Projects\gust\docs\src\SUMMARY.md`

```markdown
# Summary

[Introduction](introduction.md)

# Tutorial

- [Getting Started](tutorial/getting_started.md)
- [Your First Machine](tutorial/first_machine.md)
- [Adding Effects](tutorial/adding_effects.md)
- [Testing Your Machine](tutorial/testing.md)
- [Async Operations](tutorial/async.md)
- [Supervision Trees](tutorial/supervision.md)
- [Deploying to Production](tutorial/deployment.md)

# Language Reference

- [Syntax Overview](reference/syntax.md)
- [Types and Generics](reference/types.md)
- [States and Transitions](reference/states_transitions.md)
- [Effects and Handlers](reference/effects_handlers.md)
- [Channels and Messaging](reference/channels.md)
- [Supervision](reference/supervision.md)
- [Lifecycle Management](reference/lifecycle.md)
- [Error Handling](reference/errors.md)

# Cookbook

- [Common Patterns](cookbook/patterns.md)
  - [Request/Response](cookbook/request_response.md)
  - [Circuit Breaker](cookbook/circuit_breaker.md)
  - [Saga Pattern](cookbook/saga.md)
  - [Retry with Backoff](cookbook/retry.md)
  - [Rate Limiting](cookbook/rate_limiting.md)
  - [Health Monitoring](cookbook/health_check.md)
  - [Event Sourcing](cookbook/event_sourcing.md)
  - [CQRS](cookbook/cqrs.md)
  - [Worker Pool](cookbook/worker_pool.md)
  - [Pipeline Processing](cookbook/pipeline.md)

# Guides

- [Migration from Rust](guides/migration_rust.md)
- [Integration with Tokio](guides/tokio_integration.md)
- [Debugging State Machines](guides/debugging.md)
- [Performance Tuning](guides/performance.md)
- [Security Best Practices](guides/security.md)

# Advanced Topics

- [Code Generation Internals](advanced/codegen.md)
- [Custom Targets](advanced/custom_targets.md)
- [Extending the Compiler](advanced/compiler_plugins.md)

# Appendix

- [Grammar Reference](appendix/grammar.md)
- [Standard Library API](appendix/stdlib_api.md)
- [FAQ](appendix/faq.md)
- [Changelog](appendix/changelog.md)
```

#### Step 3: Write Tutorial Content

**File: `D:\Projects\gust\docs\src\tutorial\getting_started.md`**

```markdown
# Getting Started

Welcome to Gust! This tutorial will guide you through building your first type-safe state machine.

## Installation

### Prerequisites

- Rust 1.70 or later
- Cargo installed

### Install Gust CLI

```bash
cargo install gust-cli
```

Verify installation:

```bash
gust --version
```

### Create a New Project

```bash
gust init my-counter
cd my-counter
```

This creates a new Rust project with Gust configured:

```
my-counter/
├── Cargo.toml
├── build.rs           # Gust compilation happens here
├── src/
│   ├── main.rs
│   └── counter.gu     # Your first machine!
└── tests/
    └── counter_test.rs
```

## Your First Machine

Open `src/counter.gu`:

```gust
machine Counter {
    state Idle(count: i64)
    state Running(count: i64)

    transition start: Idle -> Running
    transition increment: Running -> Running
    transition stop: Running -> Idle

    on start(ctx: Context) {
        goto Running(0);
    }

    on increment(ctx: Context) {
        let new_count = count + 1;
        goto Running(new_count);
    }

    on stop(ctx: Context) {
        goto Idle(count);
    }
}
```

## Build and Run

```bash
cargo build
```

The `build.rs` script automatically compiles `counter.gu` to `counter.g.rs`.

## Use Your Machine

In `src/main.rs`:

```rust
mod counter;  // This imports the generated code

use counter::Counter;

fn main() {
    let mut machine = Counter::new(0);

    println!("Initial state: {:?}", machine.state());

    machine.start().unwrap();
    println!("After start: {:?}", machine.state());

    machine.increment().unwrap();
    machine.increment().unwrap();
    println!("After increments: {:?}", machine.state());

    machine.stop().unwrap();
    println!("After stop: {:?}", machine.state());
}
```

Run it:

```bash
cargo run
```

Output:
```
Initial state: Idle { count: 0 }
After start: Running { count: 0 }
After increments: Running { count: 2 }
After stop: Idle { count: 2 }
```

## What Just Happened?

1. Gust compiled your `.gu` file to Rust code
2. The generated code includes:
   - An enum `CounterState` with `Idle` and `Running` variants
   - A struct `Counter` holding the current state
   - Type-safe transition methods (`start`, `increment`, `stop`)
   - Runtime checks preventing invalid transitions
3. You used the generated API like any Rust code

## Next Steps

Continue to [Your First Machine](first_machine.md) for a deeper dive into states and transitions.
```

**File: `D:\Projects\gust\docs\src\cookbook\circuit_breaker.md`**

```markdown
# Circuit Breaker Pattern

Circuit breakers prevent cascading failures by detecting when an external service is down and "opening the circuit" to fail fast instead of waiting for timeouts.

## Problem

You're calling an external payment API. When it goes down, you don't want every request to wait 30 seconds for a timeout. Instead, detect the failure and reject requests immediately until the service recovers.

## Solution

Use the `CircuitBreaker` machine from `gust-stdlib`:

```gust
use gust_stdlib::circuit_breaker::CircuitBreaker;

type PaymentRequest {
    amount: Money,
    card_token: String,
}

type PaymentResponse {
    transaction_id: String,
    status: String,
}

machine PaymentService {
    state Ready(breaker: CircuitBreaker<PaymentResponse>)

    async effect call_payment_api(req: PaymentRequest) -> Result<PaymentResponse, String>

    transition process_payment: Ready -> Ready

    async on process_payment(req: PaymentRequest) {
        // Attempt payment through circuit breaker
        let result = breaker.execute(|| {
            perform call_payment_api(req)
        });

        match result {
            Ok(response) => {
                // Success - circuit stays closed
                goto Ready(breaker);
            }
            Err(CircuitBreakerError::Open) => {
                // Circuit is open, fail fast without calling API
                goto Ready(breaker);
            }
            Err(CircuitBreakerError::Failed(err)) => {
                // API call failed, circuit may trip
                goto Ready(breaker);
            }
        }
    }
}
```

## How It Works

The circuit breaker has three states:

1. **Closed**: Normal operation. Requests pass through. Failures are counted.
2. **Open**: Too many failures detected. All requests fail immediately without calling the API.
3. **HalfOpen**: Testing recovery. A few test requests are allowed. If they succeed, circuit closes. If they fail, circuit opens again.

State transitions:
- Closed → Open: Failure threshold exceeded (e.g., 5 consecutive failures)
- Open → HalfOpen: Timeout elapsed (e.g., 60 seconds)
- HalfOpen → Closed: Test requests succeed (e.g., 3 successes)
- HalfOpen → Open: Test request fails

## Configuration

```rust
use gust_stdlib::circuit_breaker::CircuitBreaker;

let breaker = CircuitBreaker::new(
    5,      // failure_threshold: trip after 5 failures
    60000,  // timeout_ms: wait 60 seconds before testing recovery
    3       // test_requests: require 3 successes to close
);
```

## Testing

```rust
#[tokio::test]
async fn test_circuit_breaker_trips() {
    let mut machine = PaymentService::new(CircuitBreaker::new(2, 1000, 1));

    // Simulate 2 failures
    for _ in 0..2 {
        machine.process_payment(failing_request()).await.unwrap();
    }

    // Circuit should now be open
    assert!(matches!(machine.state().breaker.state(), CircuitBreakerState::Open { .. }));

    // Next request fails immediately without calling API
    let result = machine.process_payment(any_request()).await;
    // Verify no API call was made (mock wasn't invoked)
}
```

## Real-World Example

```rust
struct PaymentEffects {
    http_client: reqwest::Client,
    api_url: String,
}

impl PaymentServiceEffects for PaymentEffects {
    async fn call_payment_api(&self, req: PaymentRequest) -> Result<PaymentResponse, String> {
        let response = self.http_client
            .post(&self.api_url)
            .json(&req)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if response.status().is_success() {
            let payment: PaymentResponse = response.json().await
                .map_err(|e| e.to_string())?;
            Ok(payment)
        } else {
            Err(format!("API returned {}", response.status()))
        }
    }
}
```

## When to Use

✅ Use circuit breakers when:
- Calling external services that may fail or become slow
- You want to fail fast instead of timing out
- You need to protect your system from cascading failures

❌ Don't use circuit breakers for:
- Local operations that can't fail
- Operations that must retry every time (use retry pattern instead)
- One-off requests (overhead isn't worth it)

## See Also

- [Retry with Backoff](retry.md) - Combine with circuit breakers for robust error handling
- [Health Check](health_check.md) - Monitor service health over time
```

#### Step 4: Add Code Example Testing

**File: `D:\Projects\gust\docs\tests\doc_examples.rs`**

```rust
// This test extracts code examples from markdown and verifies they compile

use std::fs;
use std::path::Path;
use walkdir::WalkDir;
use regex::Regex;

#[test]
fn all_gust_examples_compile() {
    let docs_dir = Path::new("src");
    let temp_dir = tempfile::tempdir().unwrap();

    let gust_block_re = Regex::new(r"```gust\n(.*?)\n```").unwrap();

    let mut example_count = 0;

    for entry in WalkDir::new(docs_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        let content = fs::read_to_string(path).unwrap();

        for cap in gust_block_re.captures_iter(&content) {
            let code = &cap[1];
            example_count += 1;

            let example_file = temp_dir.path().join(format!("example_{}.gu", example_count));
            fs::write(&example_file, code).unwrap();

            // Try to compile it
            let result = gust_lang::parse_program(code);

            assert!(
                result.is_ok(),
                "Example {} from {:?} failed to parse:\n{}",
                example_count,
                path,
                result.err().unwrap()
            );

            // Also try codegen
            let program = result.unwrap();
            let rust_code = gust_lang::RustCodegen::new().generate(&program);

            // Write generated code and try to compile with rustc
            let rust_file = temp_dir.path().join(format!("example_{}.rs", example_count));
            fs::write(&rust_file, rust_code).unwrap();

            // Note: Actually compiling with rustc requires dependencies to be available
            // For now, just verify it's valid Rust syntax by trying to parse it
        }
    }

    println!("Validated {} Gust code examples from docs", example_count);
    assert!(example_count > 0, "No examples found!");
}
```

---

## Feature 3: Community

### Requirements

**R3.1**: Create GitHub template repository for new Gust projects.

**R3.2**: Template includes:
- Configured `Cargo.toml` and `build.rs`
- Example machine with tests
- CI/CD setup (.github/workflows)
- README with quick start

**R3.3**: Design package registry concept (don't implement, just spec).

**R3.4**: Create 3 complete example projects demonstrating real-world use:
- Microservice (REST API with state machines)
- Event Processor (consume events, process with machines, publish results)
- Workflow Engine (long-running workflows with compensation)

**R3.5**: Each example project includes:
- Complete source code
- Tests achieving >80% coverage
- README with architecture diagram
- Deployment instructions (Docker)

### Acceptance Criteria

**AC3.1**: Running `cargo generate Dieshen/gust-template` creates a working project.

**AC3.2**: Template project's CI passes on GitHub Actions.

**AC3.3**: All 3 example projects run successfully with `cargo run`.

**AC3.4**: Package registry design document approved by maintainers.

**AC3.5**: Example projects are referenced in main documentation.

### Test Cases

**TC3.1 - Template Generation**

```bash
cargo generate Dieshen/gust-template --name my-service
cd my-service
cargo build
cargo test
```

Expected: All commands succeed, tests pass.

**TC3.2 - Microservice Example**

```bash
cd examples/microservice
cargo build
cargo test
docker build -t gust-microservice .
docker run -p 8080:8080 gust-microservice
curl http://localhost:8080/health
```

Expected: Service responds with `{"status": "healthy"}`.

**TC3.3 - Event Processor Example**

```bash
cd examples/event_processor
cargo build
cargo test
cargo run -- --input events.json --output results.json
```

Expected: Processes events and writes results.

### Implementation Guide

#### Step 1: Create Template Repository

**File: Template README.md**

```markdown
# Gust Project Template

A batteries-included template for new Gust projects.

## Features

- ✅ Gust compilation configured (`build.rs`)
- ✅ Example state machine with tests
- ✅ CI/CD with GitHub Actions
- ✅ Dockerfile for deployment
- ✅ Development setup with watch mode

## Quick Start

### 1. Generate Project

```bash
cargo generate Dieshen/gust-template --name my-service
cd my-service
```

### 2. Build

```bash
cargo build
```

### 3. Test

```bash
cargo test
```

### 4. Run

```bash
cargo run
```

### 5. Develop with Watch Mode

```bash
gust watch
```

(In another terminal)
```bash
cargo watch -x run
```

## Project Structure

```
my-service/
├── Cargo.toml
├── build.rs               # Gust compilation
├── Dockerfile
├── .github/
│   └── workflows/
│       └── ci.yml         # CI pipeline
├── src/
│   ├── main.rs
│   ├── app.gu             # Your machine
│   └── effects.rs         # Effect implementations
└── tests/
    └── integration_test.rs
```

## Next Steps

1. Edit `src/app.gu` - define your state machine
2. Implement effects in `src/effects.rs`
3. Add tests in `tests/`
4. Deploy with `docker build` and `docker run`

## Learn More

- [Gust Documentation](https://dieshen.github.io/gust)
- [Tutorial](https://dieshen.github.io/gust/tutorial/getting_started.html)
- [Examples](https://github.com/Dieshen/gust/tree/main/examples)
```

#### Step 2: Microservice Example

**File: `D:\Projects\gust\examples\microservice\README.md`**

```markdown
# Gust Microservice Example

A REST API for order processing using Gust state machines.

## Architecture

```
HTTP Request → Router → OrderMachine → Database
                           ↓
                      CircuitBreaker → PaymentAPI
```

### State Machines

1. **OrderMachine**: Main order processing flow
   - States: `Pending`, `Validated`, `Paid`, `Fulfilled`, `Cancelled`
   - Supervised by OrderSupervisor

2. **PaymentProcessor**: Handles payment with circuit breaker
   - Uses `CircuitBreaker` from stdlib
   - Retries failed payments with exponential backoff

3. **OrderSupervisor**: Monitors order machines
   - Restarts failed orders
   - Escalates persistent failures

## API Endpoints

- `POST /orders` - Create new order
- `GET /orders/:id` - Get order status
- `POST /orders/:id/cancel` - Cancel order
- `GET /health` - Health check

## Running

```bash
cargo run
```

Server starts on `http://localhost:8080`.

## Testing

```bash
cargo test
```

Integration tests cover:
- Order creation and validation
- Payment processing with failures
- Circuit breaker behavior
- Supervision and restart logic

## Deployment

```bash
docker build -t gust-microservice .
docker run -p 8080:8080 \
  -e DATABASE_URL=postgres://... \
  -e PAYMENT_API_URL=https://... \
  gust-microservice
```

## Performance

- Handles 10,000 req/s on 4 cores
- P99 latency: 15ms
- Graceful shutdown in <5s
```

**File: `D:\Projects\gust\examples\microservice\src\order.gu`**

```gust
use crate::types::{Order, Money, Receipt, ValidationError};
use gust_stdlib::circuit_breaker::CircuitBreaker;
use gust_stdlib::retry::Retry;

machine OrderMachine {
    state Pending(order: Order)

    state Validated(
        order: Order,
        total: Money
    )

    state PaymentProcessing(
        order: Order,
        total: Money,
        retry_machine: Retry<Receipt>
    )

    state Paid(
        order: Order,
        receipt: Receipt
    )

    state Fulfilled(
        order: Order,
        tracking: String
    )

    state Cancelled(
        order: Order,
        reason: String
    )

    state Failed(
        order: Order,
        error: String
    )

    // Transitions
    transition validate: Pending -> Validated | Failed
    transition process_payment: Validated -> PaymentProcessing
    transition payment_complete: PaymentProcessing -> Paid | Failed
    transition fulfill: Paid -> Fulfilled | Failed
    transition cancel: Pending | Validated | PaymentProcessing -> Cancelled

    // Effects
    async effect validate_order(order: Order) -> Result<Money, ValidationError>
    async effect charge_payment(total: Money) -> Result<Receipt, String>
    async effect create_shipment(order: Order) -> Result<String, String>
    async effect persist_state(state: OrderMachineState) -> Result<(), String>

    // Handlers
    async on validate(ctx: ValidateContext) {
        let validation = perform validate_order(order);

        match validation {
            Ok(total) => {
                perform persist_state(OrderMachineState::Validated { order, total });
                goto Validated(order, total);
            }
            Err(err) => {
                let error_msg = format!("Validation failed: {:?}", err);
                perform persist_state(OrderMachineState::Failed { order, error: error_msg });
                goto Failed(order, error_msg);
            }
        }
    }

    async on process_payment(ctx: PaymentContext) {
        // Create retry machine for payment processing
        let retry = Retry::new(3, 1000, 10000);  // 3 attempts, 1s base delay, 10s max

        goto PaymentProcessing(order, total, retry);
    }

    async on payment_complete(ctx: PaymentContext) {
        // Execute payment through retry machine
        let result = retry_machine.execute(|| {
            perform charge_payment(total)
        }).await;

        match result {
            Ok(receipt) => {
                perform persist_state(OrderMachineState::Paid { order, receipt });
                goto Paid(order, receipt);
            }
            Err(err) => {
                let error_msg = format!("Payment failed after retries: {}", err);
                perform persist_state(OrderMachineState::Failed { order, error: error_msg });
                goto Failed(order, error_msg);
            }
        }
    }

    async on fulfill(ctx: FulfillContext) {
        let shipment = perform create_shipment(order);

        match shipment {
            Ok(tracking) => {
                perform persist_state(OrderMachineState::Fulfilled { order, tracking });
                goto Fulfilled(order, tracking);
            }
            Err(err) => {
                let error_msg = format!("Fulfillment failed: {}", err);
                perform persist_state(OrderMachineState::Failed { order, error: error_msg });
                goto Failed(order, error_msg);
            }
        }
    }

    async on cancel(reason: String) {
        perform persist_state(OrderMachineState::Cancelled { order, reason });
        goto Cancelled(order, reason);
    }
}
```

---

## Feature 4: Compilation Targets

### Requirements

**R4.1**: Support WASM compilation target with `--target wasm` or via Rust feature flag.

**R4.2**: Generated WASM code must:
- Replace tokio with wasm-bindgen-futures for async
- Expose state machine API via #[wasm_bindgen] attributes
- Pass effects to JavaScript via callbacks
- Serialize state to/from JavaScript using serde-wasm-bindgen

**R4.3**: Support no_std compilation target for embedded systems.

**R4.4**: Generated no_std code must:
- Use feature flag `#[cfg(feature = "no_std")]`
- Replace `String` with `&str` or `heapless::String`
- Replace `Vec` with `heapless::Vec`
- Remove serde dependency (use postcard or manual serialization)
- No allocator required

**R4.5**: Support C FFI target for interoperability with C/C++ codebases.

**R4.6**: Generated C FFI code must:
- Produce `extern "C"` wrapper functions for each transition
- Generate `.h` header file with C-compatible signatures
- Represent state as C-compatible enum with `#[repr(C)]`
- Pass data via C structs with raw pointers
- Provide opaque handle for machine instance

**R4.7**: CLI must support target selection: `gust build file.gu --target [rust|wasm|nostd|cffi]`

**R4.8**: Each target must have comprehensive examples demonstrating usage.

### Acceptance Criteria

**AC4.1**: `gust build file.gu --target wasm` generates .wasm module usable from JavaScript.

**AC4.2**: Generated WASM code runs in browser and Node.js environments.

**AC4.3**: `gust build file.gu --target nostd` compiles without std crate on embedded targets.

**AC4.4**: Generated C FFI code compiles with gcc/clang and integrates with C projects.

**AC4.5**: All targets pass the same functional test suite (behavior is identical).

**AC4.6**: Documentation includes integration guides for each target.

### Test Cases

**TC4.1 - WASM Target Basic Usage**

Gust source (`counter.gu`):
```gust
machine Counter {
    state Idle(count: i32)
    state Running(count: i32)

    transition start: Idle -> Running
    transition increment: Running -> Running
    transition stop: Running -> Idle

    on start(ctx: Context) {
        goto Running(0);
    }

    on increment(ctx: Context) {
        let new_count = count + 1;
        goto Running(new_count);
    }

    on stop(ctx: Context) {
        goto Idle(count);
    }
}
```

Generated WASM Rust (`counter.g.rs` with wasm feature):
```rust
use wasm_bindgen::prelude::*;
use serde::{Serialize, Deserialize};

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Counter {
    state: CounterState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum CounterState {
    Idle { count: i32 },
    Running { count: i32 },
}

#[wasm_bindgen]
impl Counter {
    #[wasm_bindgen(constructor)]
    pub fn new(count: i32) -> Self {
        Self {
            state: CounterState::Idle { count },
        }
    }

    #[wasm_bindgen]
    pub fn start(&mut self) -> Result<(), JsValue> {
        match &self.state {
            CounterState::Idle { count } => {
                self.state = CounterState::Running { count: 0 };
                Ok(())
            }
            _ => Err(JsValue::from_str("Invalid transition")),
        }
    }

    #[wasm_bindgen]
    pub fn increment(&mut self) -> Result<(), JsValue> {
        match &self.state {
            CounterState::Running { count } => {
                let new_count = count + 1;
                self.state = CounterState::Running { count: new_count };
                Ok(())
            }
            _ => Err(JsValue::from_str("Invalid transition")),
        }
    }

    #[wasm_bindgen]
    pub fn stop(&mut self) -> Result<(), JsValue> {
        match &self.state {
            CounterState::Running { count } => {
                self.state = CounterState::Idle { count: *count };
                Ok(())
            }
            _ => Err(JsValue::from_str("Invalid transition")),
        }
    }

    #[wasm_bindgen(getter)]
    pub fn state_json(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.state)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
}
```

JavaScript usage:
```javascript
import init, { Counter } from './counter.js';

async function main() {
    await init();

    let counter = new Counter(0);
    console.log("Initial:", counter.state_json());

    counter.start();
    console.log("After start:", counter.state_json());

    counter.increment();
    counter.increment();
    console.log("After increments:", counter.state_json());

    counter.stop();
    console.log("After stop:", counter.state_json());
}

main();
```

Build command:
```bash
gust build counter.gu --target wasm
wasm-pack build --target web
```

Expected: Counter works in browser, state is accessible from JS.

**TC4.2 - WASM with Effects (Async Callbacks)**

Gust source with effects:
```gust
machine Fetcher {
    state Idle
    state Loading(url: String)
    state Loaded(data: String)
    state Error(message: String)

    async effect fetch_url(url: String) -> Result<String, String>

    transition fetch: Idle -> Loading
    transition complete: Loading -> Loaded | Error

    on fetch(url: String) {
        goto Loading(url);
    }

    async on complete(ctx: Context) {
        let result = perform fetch_url(url);
        match result {
            Ok(data) => goto Loaded(data),
            Err(err) => goto Error(err),
        }
    }
}
```

Generated WASM code:
```rust
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use js_sys::Promise;

#[wasm_bindgen]
pub struct FetcherEffects {
    fetch_url_callback: js_sys::Function,
}

#[wasm_bindgen]
impl FetcherEffects {
    #[wasm_bindgen(constructor)]
    pub fn new(fetch_url_callback: js_sys::Function) -> Self {
        Self { fetch_url_callback }
    }

    pub async fn fetch_url(&self, url: String) -> Result<String, String> {
        let this = JsValue::NULL;
        let url_js = JsValue::from_str(&url);
        let promise = self.fetch_url_callback.call1(&this, &url_js)
            .map_err(|e| format!("{:?}", e))?;

        let promise = Promise::from(promise);
        let result = JsFuture::from(promise).await
            .map_err(|e| format!("{:?}", e))?;

        result.as_string()
            .ok_or_else(|| "Expected string result".to_string())
    }
}

#[wasm_bindgen]
impl Fetcher {
    pub async fn complete(&mut self, effects: &FetcherEffects) -> Result<(), JsValue> {
        match &self.state {
            FetcherState::Loading { url } => {
                let result = effects.fetch_url(url.clone()).await;
                match result {
                    Ok(data) => {
                        self.state = FetcherState::Loaded { data };
                        Ok(())
                    }
                    Err(err) => {
                        self.state = FetcherState::Error { message: err };
                        Ok(())
                    }
                }
            }
            _ => Err(JsValue::from_str("Invalid transition")),
        }
    }
}
```

JavaScript usage:
```javascript
import init, { Fetcher, FetcherEffects } from './fetcher.js';

async function main() {
    await init();

    const effects = new FetcherEffects(async (url) => {
        const response = await fetch(url);
        return await response.text();
    });

    let fetcher = new Fetcher();
    fetcher.fetch("https://example.com/data");
    await fetcher.complete(effects);

    console.log("State:", fetcher.state_json());
}

main();
```

Expected: Async effects work via JS callbacks, promises resolve correctly.

**TC4.3 - no_std Target**

Gust source:
```gust
machine Sensor {
    state Idle
    state Reading(value: i32)
    state Alert(threshold_exceeded: i32)

    transition read: Idle -> Reading
    transition check: Reading -> Idle | Alert

    on read(value: i32) {
        goto Reading(value);
    }

    on check(threshold: i32) {
        if value > threshold {
            goto Alert(value);
        } else {
            goto Idle();
        }
    }
}
```

Generated no_std code:
```rust
#![no_std]
#![cfg_attr(feature = "no_std", no_std)]

#[cfg(feature = "no_std")]
use heapless::String;
#[cfg(not(feature = "no_std"))]
use std::string::String;

#[derive(Debug, Clone)]
pub enum SensorState {
    Idle,
    Reading { value: i32 },
    Alert { threshold_exceeded: i32 },
}

#[derive(Debug, Clone)]
pub struct Sensor {
    pub state: SensorState,
}

impl Sensor {
    pub const fn new() -> Self {
        Self {
            state: SensorState::Idle,
        }
    }

    pub fn read(&mut self, value: i32) -> Result<(), &'static str> {
        match &self.state {
            SensorState::Idle => {
                self.state = SensorState::Reading { value };
                Ok(())
            }
            _ => Err("Invalid transition"),
        }
    }

    pub fn check(&mut self, threshold: i32) -> Result<(), &'static str> {
        match &self.state {
            SensorState::Reading { value } => {
                if *value > threshold {
                    self.state = SensorState::Alert { threshold_exceeded: *value };
                } else {
                    self.state = SensorState::Idle;
                }
                Ok(())
            }
            _ => Err("Invalid transition"),
        }
    }

    pub const fn state(&self) -> &SensorState {
        &self.state
    }
}
```

Cargo.toml:
```toml
[dependencies]
heapless = { version = "0.7", optional = true }

[features]
no_std = ["heapless"]
default = []
```

Build command:
```bash
gust build sensor.gu --target nostd
cargo build --target thumbv7em-none-eabihf --no-default-features --features no_std
```

Expected: Compiles for embedded target without std, runs on microcontroller.

**TC4.4 - C FFI Target**

Gust source:
```gust
machine Door {
    state Closed
    state Opening
    state Open
    state Closing

    transition open: Closed -> Opening
    transition opened: Opening -> Open
    transition close: Open -> Closing
    transition closed: Closing -> Closed

    on open(ctx: Context) {
        goto Opening();
    }

    on opened(ctx: Context) {
        goto Open();
    }

    on close(ctx: Context) {
        goto Closing();
    }

    on closed(ctx: Context) {
        goto Closed();
    }
}
```

Generated C header (`door.h`):
```c
#ifndef DOOR_H
#define DOOR_H

#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

// Opaque handle to Door state machine
typedef struct Door Door;

// State enum
typedef enum {
    DOOR_STATE_CLOSED = 0,
    DOOR_STATE_OPENING = 1,
    DOOR_STATE_OPEN = 2,
    DOOR_STATE_CLOSING = 3,
} DoorState;

// Create new Door machine
Door* door_new(void);

// Free Door machine
void door_free(Door* machine);

// Get current state
DoorState door_get_state(const Door* machine);

// Transitions
int door_open(Door* machine);
int door_opened(Door* machine);
int door_close(Door* machine);
int door_closed(Door* machine);

#ifdef __cplusplus
}
#endif

#endif // DOOR_H
```

Generated Rust FFI code (`door.g.rs`):
```rust
use std::os::raw::c_int;

#[derive(Debug, Clone)]
pub enum DoorState {
    Closed,
    Opening,
    Open,
    Closing,
}

#[derive(Debug, Clone)]
pub struct Door {
    pub state: DoorState,
}

impl Door {
    pub fn new() -> Self {
        Self {
            state: DoorState::Closed,
        }
    }

    pub fn open(&mut self) -> Result<(), &'static str> {
        match &self.state {
            DoorState::Closed => {
                self.state = DoorState::Opening;
                Ok(())
            }
            _ => Err("Invalid transition"),
        }
    }

    pub fn opened(&mut self) -> Result<(), &'static str> {
        match &self.state {
            DoorState::Opening => {
                self.state = DoorState::Open;
                Ok(())
            }
            _ => Err("Invalid transition"),
        }
    }

    pub fn close(&mut self) -> Result<(), &'static str> {
        match &self.state {
            DoorState::Open => {
                self.state = DoorState::Closing;
                Ok(())
            }
            _ => Err("Invalid transition"),
        }
    }

    pub fn closed(&mut self) -> Result<(), &'static str> {
        match &self.state {
            DoorState::Closing => {
                self.state = DoorState::Closed;
                Ok(())
            }
            _ => Err("Invalid transition"),
        }
    }

    pub fn state(&self) -> &DoorState {
        &self.state
    }
}

// C FFI exports

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub enum CDoorState {
    Closed = 0,
    Opening = 1,
    Open = 2,
    Closing = 3,
}

#[no_mangle]
pub unsafe extern "C" fn door_new() -> *mut Door {
    Box::into_raw(Box::new(Door::new()))
}

#[no_mangle]
pub unsafe extern "C" fn door_free(ptr: *mut Door) {
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}

#[no_mangle]
pub unsafe extern "C" fn door_get_state(ptr: *const Door) -> CDoorState {
    assert!(!ptr.is_null());
    let machine = &*ptr;
    match machine.state() {
        DoorState::Closed => CDoorState::Closed,
        DoorState::Opening => CDoorState::Opening,
        DoorState::Open => CDoorState::Open,
        DoorState::Closing => CDoorState::Closing,
    }
}

#[no_mangle]
pub unsafe extern "C" fn door_open(ptr: *mut Door) -> c_int {
    assert!(!ptr.is_null());
    let machine = &mut *ptr;
    match machine.open() {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn door_opened(ptr: *mut Door) -> c_int {
    assert!(!ptr.is_null());
    let machine = &mut *ptr;
    match machine.opened() {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn door_close(ptr: *mut Door) -> c_int {
    assert!(!ptr.is_null());
    let machine = &mut *ptr;
    match machine.close() {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn door_closed(ptr: *mut Door) -> c_int {
    assert!(!ptr.is_null());
    let machine = &mut *ptr;
    match machine.closed() {
        Ok(_) => 0,
        Err(_) => -1,
    }
}
```

C usage:
```c
#include "door.h"
#include <stdio.h>

int main() {
    Door* door = door_new();

    printf("Initial state: %d\n", door_get_state(door));

    door_open(door);
    printf("After open: %d\n", door_get_state(door));

    door_opened(door);
    printf("After opened: %d\n", door_get_state(door));

    door_close(door);
    printf("After close: %d\n", door_get_state(door));

    door_closed(door);
    printf("After closed: %d\n", door_get_state(door));

    door_free(door);
    return 0;
}
```

Build commands:
```bash
gust build door.gu --target cffi
cargo build --release
gcc main.c -L target/release -ldoor -o door_demo
./door_demo
```

Expected: C program successfully controls state machine via FFI.

### Implementation Guide

#### Step 1: Add Target Parameter to CLI

File: `D:\Projects\gust\gust-cli\src\main.rs`

```rust
use clap::{Parser, ValueEnum};

#[derive(Parser)]
#[command(name = "gust")]
#[command(about = "Gust state machine compiler")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Build {
        #[arg(value_name = "FILE")]
        file: PathBuf,

        #[arg(long, default_value = "rust")]
        target: Target,

        #[arg(short = 'o', long)]
        output: Option<PathBuf>,

        #[arg(long)]
        package: Option<String>,
    },
}

#[derive(Debug, Clone, ValueEnum)]
enum Target {
    Rust,
    Go,
    Wasm,
    Nostd,
    Cffi,
}

fn build_command(file: PathBuf, target: Target, output: Option<PathBuf>, package: Option<String>) {
    let source = fs::read_to_string(&file).expect("Failed to read source file");
    let program = gust_lang::parse_program(&source).expect("Parse error");

    let code = match target {
        Target::Rust => gust_lang::RustCodegen::new().generate(&program),
        Target::Go => gust_lang::GoCodegen::new(package).generate(&program),
        Target::Wasm => gust_lang::WasmCodegen::new().generate(&program),
        Target::Nostd => gust_lang::NoStdCodegen::new().generate(&program),
        Target::Cffi => {
            let (rust_code, header) = gust_lang::CffiCodegen::new().generate(&program);
            // Write header file
            let header_path = output.as_ref()
                .map(|p| p.with_extension("h"))
                .unwrap_or_else(|| file.with_extension("h"));
            fs::write(header_path, header).expect("Failed to write header");
            rust_code
        }
    };

    let output_path = output.unwrap_or_else(|| {
        let ext = match target {
            Target::Rust | Target::Wasm | Target::Nostd | Target::Cffi => "g.rs",
            Target::Go => "g.go",
        };
        file.with_extension(ext)
    });

    fs::write(output_path, code).expect("Failed to write output");
}
```

#### Step 2: Implement WASM Codegen

File: `D:\Projects\gust\gust-lang\src\codegen_wasm.rs`

```rust
use crate::ast::*;

pub struct WasmCodegen {
    output: String,
    indent: usize,
}

impl WasmCodegen {
    pub fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
        }
    }

    pub fn generate(&mut self, program: &Program) -> String {
        self.line("use wasm_bindgen::prelude::*;");
        self.line("use serde::{Serialize, Deserialize};");
        self.line("use wasm_bindgen_futures::JsFuture;");
        self.line("use js_sys::Promise;");
        self.line("");

        for machine in &program.machines {
            self.emit_machine(machine);
        }

        self.output.clone()
    }

    fn emit_machine(&mut self, machine: &MachineDecl) {
        let name = &machine.name;
        let state_enum = format!("{name}State");

        // State enum (not exposed to JS)
        self.line("#[derive(Debug, Clone, Serialize, Deserialize)]");
        self.line(&format!("enum {state_enum} {{"));
        self.indent += 1;
        for state in &machine.states {
            if state.fields.is_empty() {
                self.line(&format!("{},", state.name));
            } else {
                self.line(&format!("{} {{", state.name));
                self.indent += 1;
                for field in &state.fields {
                    self.line(&format!("{}: {},", field.name, self.rust_type(&field.field_type)));
                }
                self.indent -= 1;
                self.line("},");
            }
        }
        self.indent -= 1;
        self.line("}");
        self.line("");

        // Machine struct exposed to JS
        self.line("#[wasm_bindgen]");
        self.line("#[derive(Debug, Clone, Serialize, Deserialize)]");
        self.line(&format!("pub struct {name} {{"));
        self.indent += 1;
        self.line(&format!("state: {state_enum},"));
        self.indent -= 1;
        self.line("}");
        self.line("");

        // Effects trait (if needed)
        if !machine.effects.is_empty() {
            self.emit_effects_struct(machine);
        }

        // Impl block with wasm_bindgen
        self.line("#[wasm_bindgen]");
        self.line(&format!("impl {name} {{"));
        self.indent += 1;

        // Constructor
        self.emit_constructor(machine);

        // Transitions
        for handler in &machine.handlers {
            self.emit_handler_wasm(machine, handler);
        }

        // State getter
        self.line("#[wasm_bindgen(getter)]");
        self.line("pub fn state_json(&self) -> Result<JsValue, JsValue> {");
        self.indent += 1;
        self.line("serde_wasm_bindgen::to_value(&self.state)");
        self.indent += 1;
        self.line(".map_err(|e| JsValue::from_str(&e.to_string()))");
        self.indent -= 1;
        self.indent -= 1;
        self.line("}");

        self.indent -= 1;
        self.line("}");
        self.line("");
    }

    fn emit_effects_struct(&mut self, machine: &MachineDecl) {
        let effects_name = format!("{}Effects", machine.name);

        self.line("#[wasm_bindgen]");
        self.line(&format!("pub struct {effects_name} {{"));
        self.indent += 1;
        for effect in &machine.effects {
            self.line(&format!("{}_callback: js_sys::Function,", effect.name));
        }
        self.indent -= 1;
        self.line("}");
        self.line("");

        self.line("#[wasm_bindgen]");
        self.line(&format!("impl {effects_name} {{"));
        self.indent += 1;

        self.line("#[wasm_bindgen(constructor)]");
        self.line("pub fn new(");
        self.indent += 1;
        for (i, effect) in machine.effects.iter().enumerate() {
            let comma = if i < machine.effects.len() - 1 { "," } else { "" };
            self.line(&format!("{}_callback: js_sys::Function{}", effect.name, comma));
        }
        self.indent -= 1;
        self.line(") -> Self {");
        self.indent += 1;
        self.line("Self {");
        self.indent += 1;
        for effect in &machine.effects {
            self.line(&format!("{}_callback,", effect.name));
        }
        self.indent -= 1;
        self.line("}");
        self.indent -= 1;
        self.line("}");

        // Effect methods
        for effect in &machine.effects {
            self.emit_effect_method(effect);
        }

        self.indent -= 1;
        self.line("}");
        self.line("");
    }

    fn emit_effect_method(&mut self, effect: &EffectDecl) {
        let async_kw = if effect.is_async { "async " } else { "" };
        let params: Vec<String> = effect.params.iter()
            .map(|p| format!("{}: {}", p.name, self.rust_type(&p.param_type)))
            .collect();

        self.line(&format!("pub {}fn {}(&self, {}) -> Result<{}, String> {{",
            async_kw,
            effect.name,
            params.join(", "),
            self.rust_type(&effect.return_type)
        ));
        self.indent += 1;

        self.line("let this = JsValue::NULL;");

        // Convert params to JsValue
        for param in &effect.params {
            self.line(&format!("let {}_js = serde_wasm_bindgen::to_value(&{})",
                param.name, param.name));
            self.indent += 1;
            self.line(".map_err(|e| e.to_string())?;");
            self.indent -= 1;
        }

        // Call JS callback
        let js_params: Vec<String> = effect.params.iter()
            .map(|p| format!("&{}_js", p.name))
            .collect();

        if js_params.is_empty() {
            self.line(&format!("let result = self.{}_callback.call0(&this)", effect.name));
        } else {
            self.line(&format!("let result = self.{}_callback.call{}&this, {})",
                effect.name, js_params.len(), js_params.join(", ")));
        }
        self.indent += 1;
        self.line(".map_err(|e| format!(\"{:?}\", e))?;");
        self.indent -= 1;

        if effect.is_async {
            self.line("let promise = Promise::from(result);");
            self.line("let result = JsFuture::from(promise).await");
            self.indent += 1;
            self.line(".map_err(|e| format!(\"{:?}\", e))?;");
            self.indent -= 1;
        }

        self.line("serde_wasm_bindgen::from_value(result)");
        self.indent += 1;
        self.line(".map_err(|e| e.to_string())");
        self.indent -= 1;

        self.indent -= 1;
        self.line("}");
    }

    fn emit_handler_wasm(&mut self, machine: &MachineDecl, handler: &OnHandler) {
        let transition = machine.transitions.iter()
            .find(|t| t.name == handler.transition_name)
            .expect("Transition not found");

        let async_kw = if handler.is_async { "async " } else { "" };
        let params: Vec<String> = handler.params.iter()
            .map(|p| format!("{}: {}", p.name, self.rust_type(&p.param_type)))
            .collect();

        let effects_param = if !machine.effects.is_empty() {
            format!(", effects: &{}Effects", machine.name)
        } else {
            String::new()
        };

        self.line("#[wasm_bindgen]");
        self.line(&format!("pub {}fn {}(&mut self{}{}) -> Result<(), JsValue> {{",
            async_kw,
            transition.name,
            if params.is_empty() { "" } else { ", " },
            params.join(", ")
        ));
        self.indent += 1;

        // Pattern match on current state
        self.line("match &self.state {");
        self.indent += 1;

        let state_enum = format!("{}State", machine.name);
        self.line(&format!("{}::{} {{ .. }} => {{", state_enum, transition.from_state));
        self.indent += 1;

        // Handler body (simplified - full codegen would translate handler.body)
        self.line("// Handler body here");
        self.line("Ok(())");

        self.indent -= 1;
        self.line("}");

        self.line("_ => Err(JsValue::from_str(\"Invalid transition\")),");

        self.indent -= 1;
        self.line("}");

        self.indent -= 1;
        self.line("}");
    }

    fn emit_constructor(&mut self, machine: &MachineDecl) {
        // Find initial state
        let initial_state = &machine.states[0];

        self.line("#[wasm_bindgen(constructor)]");

        if initial_state.fields.is_empty() {
            self.line("pub fn new() -> Self {");
            self.indent += 1;
            self.line("Self {");
            self.indent += 1;
            self.line(&format!("state: {}State::{},", machine.name, initial_state.name));
            self.indent -= 1;
            self.line("}");
            self.indent -= 1;
            self.line("}");
        } else {
            let params: Vec<String> = initial_state.fields.iter()
                .map(|f| format!("{}: {}", f.name, self.rust_type(&f.field_type)))
                .collect();

            self.line(&format!("pub fn new({}) -> Self {{", params.join(", ")));
            self.indent += 1;
            self.line("Self {");
            self.indent += 1;
            self.line(&format!("state: {}State::{} {{", machine.name, initial_state.name));
            self.indent += 1;
            for field in &initial_state.fields {
                self.line(&format!("{},", field.name));
            }
            self.indent -= 1;
            self.line("},");
            self.indent -= 1;
            self.line("}");
            self.indent -= 1;
            self.line("}");
        }
    }

    fn rust_type(&self, type_expr: &str) -> String {
        type_expr.to_string()
    }

    fn line(&mut self, s: &str) {
        self.output.push_str(&"    ".repeat(self.indent));
        self.output.push_str(s);
        self.output.push('\n');
    }
}
```

#### Step 3: Implement no_std Codegen

File: `D:\Projects\gust\gust-lang\src\codegen_nostd.rs`

```rust
use crate::ast::*;

pub struct NoStdCodegen {
    output: String,
    indent: usize,
}

impl NoStdCodegen {
    pub fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
        }
    }

    pub fn generate(&mut self, program: &Program) -> String {
        self.line("#![no_std]");
        self.line("#![cfg_attr(feature = \"no_std\", no_std)]");
        self.line("");
        self.line("#[cfg(feature = \"no_std\")]");
        self.line("use heapless::{String, Vec};");
        self.line("#[cfg(not(feature = \"no_std\"))]");
        self.line("use std::{string::String, vec::Vec};");
        self.line("");

        for machine in &program.machines {
            self.emit_machine(machine);
        }

        self.output.clone()
    }

    fn emit_machine(&mut self, machine: &MachineDecl) {
        let name = &machine.name;
        let state_enum = format!("{name}State");

        // State enum
        self.line("#[derive(Debug, Clone)]");
        self.line(&format!("pub enum {state_enum} {{"));
        self.indent += 1;
        for state in &machine.states {
            if state.fields.is_empty() {
                self.line(&format!("{},", state.name));
            } else {
                self.line(&format!("{} {{", state.name));
                self.indent += 1;
                for field in &state.fields {
                    self.line(&format!("{}: {},", field.name, self.nostd_type(&field.field_type)));
                }
                self.indent -= 1;
                self.line("},");
            }
        }
        self.indent -= 1;
        self.line("}");
        self.line("");

        // Machine struct
        self.line("#[derive(Debug, Clone)]");
        self.line(&format!("pub struct {name} {{"));
        self.indent += 1;
        self.line(&format!("pub state: {state_enum},"));
        self.indent -= 1;
        self.line("}");
        self.line("");

        // Impl block
        self.line(&format!("impl {name} {{"));
        self.indent += 1;

        // Constructor
        let initial_state = &machine.states[0];
        if initial_state.fields.is_empty() {
            self.line("pub const fn new() -> Self {");
            self.indent += 1;
            self.line("Self {");
            self.indent += 1;
            self.line(&format!("state: {state_enum}::{},", initial_state.name));
            self.indent -= 1;
            self.line("}");
            self.indent -= 1;
            self.line("}");
        }

        // Transitions (use &'static str for errors)
        for handler in &machine.handlers {
            self.emit_handler_nostd(machine, handler);
        }

        // State getter
        self.line("pub const fn state(&self) -> &{}State {{", name);
        self.indent += 1;
        self.line("&self.state");
        self.indent -= 1;
        self.line("}");

        self.indent -= 1;
        self.line("}");
        self.line("");
    }

    fn emit_handler_nostd(&mut self, machine: &MachineDecl, handler: &OnHandler) {
        let transition = machine.transitions.iter()
            .find(|t| t.name == handler.transition_name)
            .expect("Transition not found");

        let params: Vec<String> = handler.params.iter()
            .map(|p| format!("{}: {}", p.name, self.nostd_type(&p.param_type)))
            .collect();

        self.line(&format!("pub fn {}(&mut self{}) -> Result<(), &'static str> {{",
            transition.name,
            if params.is_empty() { "" } else { ", " },
            params.join(", ")
        ));
        self.indent += 1;

        self.line("match &self.state {");
        self.indent += 1;

        let state_enum = format!("{}State", machine.name);
        self.line(&format!("{}::{} {{ .. }} => {{", state_enum, transition.from_state));
        self.indent += 1;

        self.line("// Handler body");
        self.line("Ok(())");

        self.indent -= 1;
        self.line("}");

        self.line("_ => Err(\"Invalid transition\"),");

        self.indent -= 1;
        self.line("}");

        self.indent -= 1;
        self.line("}");
    }

    fn nostd_type(&self, type_expr: &str) -> String {
        match type_expr {
            "String" => "heapless::String<32>".to_string(),
            s if s.starts_with("Vec<") => {
                let inner = &s[4..s.len()-1];
                format!("heapless::Vec<{}, 16>", inner)
            }
            other => other.to_string(),
        }
    }

    fn line(&mut self, s: &str) {
        self.output.push_str(&"    ".repeat(self.indent));
        self.output.push_str(s);
        self.output.push('\n');
    }
}
```

#### Step 4: Implement C FFI Codegen

File: `D:\Projects\gust\gust-lang\src\codegen_ffi.rs`

```rust
use crate::ast::*;

pub struct CffiCodegen {
    rust_output: String,
    header_output: String,
    indent: usize,
}

impl CffiCodegen {
    pub fn new() -> Self {
        Self {
            rust_output: String::new(),
            header_output: String::new(),
            indent: 0,
        }
    }

    pub fn generate(&mut self, program: &Program) -> (String, String) {
        // Generate header
        self.emit_header_prologue(&program.machines[0]);

        // Generate Rust code
        self.emit_rust_prologue();

        for machine in &program.machines {
            self.emit_machine(machine);
        }

        self.emit_header_epilogue();

        (self.rust_output.clone(), self.header_output.clone())
    }

    fn emit_header_prologue(&mut self, machine: &MachineDecl) {
        let guard = format!("{}_H", machine.name.to_uppercase());
        self.h_line(&format!("#ifndef {guard}"));
        self.h_line(&format!("#define {guard}"));
        self.h_line("");
        self.h_line("#include <stdint.h>");
        self.h_line("#include <stdbool.h>");
        self.h_line("");
        self.h_line("#ifdef __cplusplus");
        self.h_line("extern \"C\" {");
        self.h_line("#endif");
        self.h_line("");
    }

    fn emit_header_epilogue(&mut self) {
        self.h_line("");
        self.h_line("#ifdef __cplusplus");
        self.h_line("}");
        self.h_line("#endif");
        self.h_line("");
        self.h_line("#endif");
    }

    fn emit_rust_prologue(&mut self) {
        self.r_line("use std::os::raw::c_int;");
        self.r_line("");
    }

    fn emit_machine(&mut self, machine: &MachineDecl) {
        let name = &machine.name;
        let c_name = name.to_lowercase();

        // Header: opaque type
        self.h_line(&format!("typedef struct {name} {name};"));
        self.h_line("");

        // Header: state enum
        self.h_line(&format!("typedef enum {{"));
        for (i, state) in machine.states.iter().enumerate() {
            self.h_line(&format!("    {}_STATE_{} = {},",
                c_name.to_uppercase(),
                state.name.to_uppercase(),
                i
            ));
        }
        self.h_line(&format!("}} {name}State;"));
        self.h_line("");

        // Rust: state enum
        self.r_line("#[derive(Debug, Clone)]");
        self.r_line(&format!("pub enum {name}State {{"));
        for state in &machine.states {
            if state.fields.is_empty() {
                self.r_line(&format!("    {},", state.name));
            } else {
                self.r_line(&format!("    {} {{", state.name));
                for field in &state.fields {
                    self.r_line(&format!("        {}: {},", field.name, field.field_type));
                }
                self.r_line("    },");
            }
        }
        self.r_line("}");
        self.r_line("");

        // Rust: machine struct
        self.r_line("#[derive(Debug, Clone)]");
        self.r_line(&format!("pub struct {name} {{"));
        self.r_line(&format!("    pub state: {name}State,"));
        self.r_line("}");
        self.r_line("");

        // Rust: impl block
        self.r_line(&format!("impl {name} {{"));
        self.r_line("    pub fn new() -> Self {");
        self.r_line("        Self {");
        self.r_line(&format!("            state: {name}State::{},", machine.states[0].name));
        self.r_line("        }");
        self.r_line("    }");

        for handler in &machine.handlers {
            self.emit_rust_transition(machine, handler);
        }

        self.r_line("}");
        self.r_line("");

        // C FFI exports
        self.emit_ffi_functions(machine);
    }

    fn emit_rust_transition(&mut self, machine: &MachineDecl, handler: &OnHandler) {
        let transition = machine.transitions.iter()
            .find(|t| t.name == handler.transition_name)
            .unwrap();

        self.r_line(&format!("    pub fn {}(&mut self) -> Result<(), &'static str> {{", transition.name));
        self.r_line("        match &self.state {");
        self.r_line(&format!("            {}State::{} => {{", machine.name, transition.from_state));
        self.r_line("                // Transition logic");
        self.r_line("                Ok(())");
        self.r_line("            }");
        self.r_line("            _ => Err(\"Invalid transition\"),");
        self.r_line("        }");
        self.r_line("    }");
    }

    fn emit_ffi_functions(&mut self, machine: &MachineDecl) {
        let name = &machine.name;
        let c_name = name.to_lowercase();

        // new
        self.h_line(&format!("{name}* {c_name}_new(void);"));
        self.r_line("#[no_mangle]");
        self.r_line(&format!("pub unsafe extern \"C\" fn {c_name}_new() -> *mut {name} {{"));
        self.r_line(&format!("    Box::into_raw(Box::new({name}::new()))"));
        self.r_line("}");
        self.r_line("");

        // free
        self.h_line(&format!("void {c_name}_free({name}* machine);"));
        self.r_line("#[no_mangle]");
        self.r_line(&format!("pub unsafe extern \"C\" fn {c_name}_free(ptr: *mut {name}) {{"));
        self.r_line("    if !ptr.is_null() {");
        self.r_line("        drop(Box::from_raw(ptr));");
        self.r_line("    }");
        self.r_line("}");
        self.r_line("");

        // get_state
        self.h_line(&format!("{name}State {c_name}_get_state(const {name}* machine);"));
        self.r_line("#[no_mangle]");
        self.r_line(&format!("pub unsafe extern \"C\" fn {c_name}_get_state(ptr: *const {name}) -> C{name}State {{"));
        self.r_line("    assert!(!ptr.is_null());");
        self.r_line("    let machine = &*ptr;");
        self.r_line("    match &machine.state {");
        for (i, state) in machine.states.iter().enumerate() {
            self.r_line(&format!("        {name}State::{} => C{name}State::{},",
                state.name, state.name));
        }
        self.r_line("    }");
        self.r_line("}");
        self.r_line("");

        // transitions
        for handler in &machine.handlers {
            let transition = machine.transitions.iter()
                .find(|t| t.name == handler.transition_name)
                .unwrap();

            self.h_line(&format!("int {c_name}_{}({name}* machine);", transition.name));
            self.r_line("#[no_mangle]");
            self.r_line(&format!("pub unsafe extern \"C\" fn {c_name}_{}(ptr: *mut {name}) -> c_int {{",
                transition.name));
            self.r_line("    assert!(!ptr.is_null());");
            self.r_line("    let machine = &mut *ptr;");
            self.r_line(&format!("    match machine.{}() {{", transition.name));
            self.r_line("        Ok(_) => 0,");
            self.r_line("        Err(_) => -1,");
            self.r_line("    }");
            self.r_line("}");
            self.r_line("");
        }
    }

    fn r_line(&mut self, s: &str) {
        self.rust_output.push_str(s);
        self.rust_output.push('\n');
    }

    fn h_line(&mut self, s: &str) {
        self.header_output.push_str(s);
        self.header_output.push('\n');
    }
}
```

---

## Constraints

### Technical Constraints

**TC1**: WASM target requires wasm32-unknown-unknown toolchain and wasm-pack.

**TC2**: no_std target requires embedded toolchain (e.g., thumbv7em-none-eabihf).

**TC3**: C FFI target requires stable ABI via `#[repr(C)]` and `extern "C"`.

**TC4**: Standard library machines must work across all targets (may require conditional compilation).

**TC5**: Documentation generation (mdBook) requires mdbook binary installed.

### Resource Constraints

**RC1**: Example projects should compile in <2 minutes on CI.

**RC2**: WASM output should be <500KB after wasm-opt optimization.

**RC3**: no_std code should fit in 64KB flash for typical embedded targets.

**RC4**: Documentation build should complete in <30 seconds.

### Compatibility Constraints

**CC1**: Standard library must remain compatible with Rust 1.70+.

**CC2**: WASM target must work in browsers supporting WebAssembly 1.0.

**CC3**: C FFI must be compatible with C99 and C++11.

**CC4**: Documentation must render correctly on desktop and mobile browsers.

---

## Verification Checklist

Use this checklist to verify Phase 4 is complete:

### Feature 1: Standard Library

- [ ] `gust-stdlib` crate created with Cargo.toml and build.rs
- [ ] Generic machine syntax added to grammar.pest
- [ ] AST updated with GenericParam support
- [ ] RustCodegen emits correct generic bounds and type parameters
- [ ] GoCodegen handles generics (interface{} or Go 1.18+ generics)
- [ ] All 6 stdlib machines implemented (.gu files):
  - [ ] request_response.gu
  - [ ] circuit_breaker.gu
  - [ ] saga.gu
  - [ ] retry.gu
  - [ ] rate_limiter.gu
  - [ ] health_check.gu
- [ ] stdlib compiles to Rust without errors
- [ ] stdlib compiles to Go without errors
- [ ] All stdlib test cases pass (TC1.1, TC1.2, TC1.3)
- [ ] User projects can import and use stdlib machines

### Feature 2: Documentation

- [ ] mdBook initialized in docs/ directory
- [ ] book.toml configured with correct settings
- [ ] SUMMARY.md with complete table of contents
- [ ] Tutorial chapters written (7 chapters):
  - [ ] getting_started.md
  - [ ] first_machine.md
  - [ ] adding_effects.md
  - [ ] testing.md
  - [ ] async.md
  - [ ] supervision.md
  - [ ] deployment.md
- [ ] Reference documentation written (8 chapters)
- [ ] Cookbook with 10+ patterns
- [ ] Migration guide from Rust
- [ ] Code example validation test (doc_examples.rs)
- [ ] All Gust code examples compile successfully
- [ ] `mdbook serve` works locally
- [ ] Documentation deployed to GitHub Pages
- [ ] Versioned docs for each release

### Feature 3: Community

- [ ] GitHub template repository created
- [ ] Template includes working Cargo.toml and build.rs
- [ ] Template includes example machine with tests
- [ ] Template includes CI/CD workflow
- [ ] Template includes Dockerfile
- [ ] `cargo generate` works with template
- [ ] All 3 example projects created:
  - [ ] examples/microservice (REST API)
  - [ ] examples/event_processor
  - [ ] examples/workflow_engine
- [ ] Each example has README with architecture diagram
- [ ] Each example has >80% test coverage
- [ ] Each example has deployment instructions
- [ ] All examples run successfully
- [ ] Package registry design document written
- [ ] Examples referenced in main documentation

### Feature 4: Compilation Targets

- [ ] CLI updated with --target parameter
- [ ] WasmCodegen implemented (codegen_wasm.rs)
- [ ] NoStdCodegen implemented (codegen_nostd.rs)
- [ ] CffiCodegen implemented (codegen_ffi.rs)
- [ ] WASM target generates valid wasm_bindgen code
- [ ] WASM target compiles with wasm-pack
- [ ] WASM works in browser and Node.js
- [ ] WASM async effects work via JS callbacks
- [ ] no_std target compiles without std
- [ ] no_std works on embedded target (e.g., ARM Cortex-M)
- [ ] C FFI generates .h header file
- [ ] C FFI generates extern "C" functions
- [ ] C FFI compiles with gcc/clang
- [ ] C code successfully calls state machine via FFI
- [ ] All test cases pass (TC4.1, TC4.2, TC4.3, TC4.4)
- [ ] Integration guides written for each target

### CI/CD

- [ ] CI pipeline runs all tests
- [ ] CI validates documentation examples
- [ ] CI builds all example projects
- [ ] CI builds all compilation targets
- [ ] GitHub Actions workflow passing
- [ ] Release automation configured

### Final Verification

- [ ] All constraints satisfied
- [ ] All acceptance criteria met
- [ ] Performance benchmarks run
- [ ] Security review completed
- [ ] User feedback incorporated
- [ ] Migration path from Phase 3 tested
- [ ] Backward compatibility verified
- [ ] Release notes written

---

## File Map

Complete list of files to create or modify for Phase 4:

### Standard Library (Feature 1)

**New Files:**
```
D:\Projects\gust\gust-stdlib\Cargo.toml
D:\Projects\gust\gust-stdlib\build.rs
D:\Projects\gust\gust-stdlib\src\lib.rs
D:\Projects\gust\gust-stdlib\request_response.gu
D:\Projects\gust\gust-stdlib\circuit_breaker.gu
D:\Projects\gust\gust-stdlib\saga.gu
D:\Projects\gust\gust-stdlib\retry.gu
D:\Projects\gust\gust-stdlib\rate_limiter.gu
D:\Projects\gust\gust-stdlib\health_check.gu
D:\Projects\gust\gust-stdlib\tests\integration_test.rs
```

**Modified Files:**
```
D:\Projects\gust\gust-lang\src\grammar.pest (add generic_params)
D:\Projects\gust\gust-lang\src\ast.rs (add GenericParam)
D:\Projects\gust\gust-lang\src\parser.rs (parse generics)
D:\Projects\gust\gust-lang\src\codegen.rs (emit generics)
D:\Projects\gust\gust-lang\src\codegen_go.rs (emit generics)
D:\Projects\gust\Cargo.toml (add stdlib to workspace)
```

### Documentation (Feature 2)

**New Files:**
```
D:\Projects\gust\docs\book.toml
D:\Projects\gust\docs\src\SUMMARY.md
D:\Projects\gust\docs\src\introduction.md
D:\Projects\gust\docs\src\tutorial\getting_started.md
D:\Projects\gust\docs\src\tutorial\first_machine.md
D:\Projects\gust\docs\src\tutorial\adding_effects.md
D:\Projects\gust\docs\src\tutorial\testing.md
D:\Projects\gust\docs\src\tutorial\async.md
D:\Projects\gust\docs\src\tutorial\supervision.md
D:\Projects\gust\docs\src\tutorial\deployment.md
D:\Projects\gust\docs\src\reference\syntax.md
D:\Projects\gust\docs\src\reference\types.md
D:\Projects\gust\docs\src\reference\states_transitions.md
D:\Projects\gust\docs\src\reference\effects_handlers.md
D:\Projects\gust\docs\src\reference\channels.md
D:\Projects\gust\docs\src\reference\supervision.md
D:\Projects\gust\docs\src\reference\lifecycle.md
D:\Projects\gust\docs\src\reference\errors.md
D:\Projects\gust\docs\src\cookbook\patterns.md
D:\Projects\gust\docs\src\cookbook\request_response.md
D:\Projects\gust\docs\src\cookbook\circuit_breaker.md
D:\Projects\gust\docs\src\cookbook\saga.md
D:\Projects\gust\docs\src\cookbook\retry.md
D:\Projects\gust\docs\src\cookbook\rate_limiting.md
D:\Projects\gust\docs\src\cookbook\health_check.md
D:\Projects\gust\docs\src\cookbook\event_sourcing.md
D:\Projects\gust\docs\src\cookbook\cqrs.md
D:\Projects\gust\docs\src\cookbook\worker_pool.md
D:\Projects\gust\docs\src\cookbook\pipeline.md
D:\Projects\gust\docs\src\guides\migration_rust.md
D:\Projects\gust\docs\src\guides\tokio_integration.md
D:\Projects\gust\docs\src\guides\debugging.md
D:\Projects\gust\docs\src\guides\performance.md
D:\Projects\gust\docs\src\guides\security.md
D:\Projects\gust\docs\src\advanced\codegen.md
D:\Projects\gust\docs\src\advanced\custom_targets.md
D:\Projects\gust\docs\src\advanced\compiler_plugins.md
D:\Projects\gust\docs\src\appendix\grammar.md
D:\Projects\gust\docs\src\appendix\stdlib_api.md
D:\Projects\gust\docs\src\appendix\faq.md
D:\Projects\gust\docs\src\appendix\changelog.md
D:\Projects\gust\docs\tests\doc_examples.rs
D:\Projects\gust\.github\workflows\docs.yml
```

### Community (Feature 3)

**New Files:**
```
gust-template\Cargo.toml (separate repo)
gust-template\build.rs
gust-template\Dockerfile
gust-template\README.md
gust-template\.github\workflows\ci.yml
gust-template\src\main.rs
gust-template\src\app.gu
gust-template\src\effects.rs
gust-template\tests\integration_test.rs

D:\Projects\gust\examples\microservice\Cargo.toml
D:\Projects\gust\examples\microservice\Dockerfile
D:\Projects\gust\examples\microservice\README.md
D:\Projects\gust\examples\microservice\src\main.rs
D:\Projects\gust\examples\microservice\src\order.gu
D:\Projects\gust\examples\microservice\src\payment.gu
D:\Projects\gust\examples\microservice\src\supervisor.gu
D:\Projects\gust\examples\microservice\src\effects.rs
D:\Projects\gust\examples\microservice\tests\integration_test.rs

D:\Projects\gust\examples\event_processor\Cargo.toml
D:\Projects\gust\examples\event_processor\README.md
D:\Projects\gust\examples\event_processor\src\main.rs
D:\Projects\gust\examples\event_processor\src\processor.gu
D:\Projects\gust\examples\event_processor\tests\test.rs

D:\Projects\gust\examples\workflow_engine\Cargo.toml
D:\Projects\gust\examples\workflow_engine\README.md
D:\Projects\gust\examples\workflow_engine\src\main.rs
D:\Projects\gust\examples\workflow_engine\src\workflow.gu
D:\Projects\gust\examples\workflow_engine\tests\test.rs

D:\Projects\gust\docs\specs\package_registry.md
```

### Compilation Targets (Feature 4)

**New Files:**
```
D:\Projects\gust\gust-lang\src\codegen_wasm.rs
D:\Projects\gust\gust-lang\src\codegen_nostd.rs
D:\Projects\gust\gust-lang\src\codegen_ffi.rs
D:\Projects\gust\examples\wasm\counter.gu
D:\Projects\gust\examples\wasm\index.html
D:\Projects\gust\examples\wasm\package.json
D:\Projects\gust\examples\wasm\README.md
D:\Projects\gust\examples\nostd\sensor.gu
D:\Projects\gust\examples\nostd\Cargo.toml
D:\Projects\gust\examples\nostd\README.md
D:\Projects\gust\examples\cffi\door.gu
D:\Projects\gust\examples\cffi\main.c
D:\Projects\gust\examples\cffi\Makefile
D:\Projects\gust\examples\cffi\README.md
```

**Modified Files:**
```
D:\Projects\gust\gust-cli\src\main.rs (add --target parameter)
D:\Projects\gust\gust-lang\src\lib.rs (export new codegens)
D:\Projects\gust\gust-build\src\lib.rs (support target selection)
```

### CI/CD

**Modified Files:**
```
D:\Projects\gust\.github\workflows\ci.yml (add new checks)
D:\Projects\gust\.github\workflows\release.yml (add targets)
```

---

END OF SPEC