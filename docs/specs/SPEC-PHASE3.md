# Phase 3 Spec: Structured Concurrency

## Prerequisites

Before starting Phase 3 implementation:

1. **Phase 1 AND Phase 2 are COMPLETE** - All async support, cargo integration, type system improvements, VS Code extension, LSP, and tooling are working.
2. **Runtime foundation exists** - `gust-runtime` already has `Machine`, `Supervisor`, and `Envelope<T>` types.
3. **Tokio runtime** - Rust 1.70+ with tokio for async concurrency primitives.
4. **Understanding of Erlang/OTP** - Familiarity with supervision trees, gen_server, and process linking patterns.

## Current State (Post-Phase 2)

### Runtime Infrastructure (gust-runtime/src/lib.rs)

The runtime already provides:

```rust
pub trait Machine: Serialize + for<'de> Deserialize<'de> {
    type State: std::fmt::Debug + Clone + Serialize + for<'de> Deserialize<'de>;
    fn current_state(&self) -> &Self::State;
    fn to_json(&self) -> Result<String, serde_json::Error>;
    fn from_json(json: &str) -> Result<Self, serde_json::Error>;
}

pub trait Supervisor {
    type Error: std::fmt::Debug;
    fn on_child_failure(&mut self, child_id: &str, error: &Self::Error) -> SupervisorAction;
}

pub enum SupervisorAction {
    Restart,    // Restart the child machine from its initial state
    Escalate,   // Stop the child and propagate the error up
    Ignore,     // Ignore the failure and continue
}

pub struct Envelope<T: Serialize> {
    pub source: String,
    pub target: String,
    pub payload: T,
    pub correlation_id: Option<String>,
}
```

### Current AST Types (Post-Phase 1)

```rust
pub struct Program {
    pub uses: Vec<UsePath>,
    pub types: Vec<TypeDecl>,
    pub machines: Vec<MachineDecl>,
}

pub struct MachineDecl {
    pub name: String,
    pub states: Vec<StateDecl>,
    pub transitions: Vec<TransitionDecl>,
    pub handlers: Vec<OnHandler>,
    pub effects: Vec<EffectDecl>,
}

pub struct OnHandler {
    pub transition_name: String,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    pub body: Block,
    pub is_async: bool,  // From Phase 1
}

pub struct EffectDecl {
    pub name: String,
    pub params: Vec<Field>,
    pub return_type: TypeExpr,
    pub is_async: bool,  // From Phase 1
}
```

### Example: OrderProcessor (examples/order_processor.gu)

The POC already includes a supervisor machine:

```gust
machine OrderSupervisor {
    state Watching(active_orders: i64)
    state Degraded(failed_count: i64)
    state Shutdown

    transition order_failed: Watching -> Watching | Degraded
    transition recover:      Degraded -> Watching
    transition kill:         Degraded -> Shutdown
}
```

Phase 3 will make this **actually supervise** child machines with real process management.

---

## Constraints

### Design Principles

**DP1: Keep It Simple** - The syntax should be as simple as Erlang's supervisor behavior, but with static types.

**DP2: Rust-Native Concurrency** - Generated code uses tokio primitives (tasks, channels, JoinSet) idiomatically. No actor framework dependencies.

**DP3: Go-Native Concurrency** - Generated Go code uses goroutines, channels, and errgroup. No third-party actor libraries.

**DP4: Fail Fast with Typed Errors** - Supervision decisions are based on typed error enums, not string matching.

**DP5: Zero-Copy In-Process** - When machines run in the same process, messages pass via direct references (no serialization).

**DP6: Transparent Serialization** - When machines run across process boundaries, the same `.gu` code auto-serializes messages.

### What NOT to Change

**NC1**: Do NOT alter existing `Machine` or `Supervisor` traits in the runtime. Extend them, don't replace.

**NC2**: Do NOT change the code generation structure for non-concurrent machines. A simple machine from Phase 1 should generate identical code.

**NC3**: Do NOT introduce a heavyweight actor framework. Use only Rust/Go standard concurrency primitives.

**NC4**: Do NOT make channels or supervision mandatory. Machines can still be standalone (backward compatible).

### Performance Requirements

**P1**: In-process message passing must be zero-copy (Arc/Rc wrappers, not clones).

**P2**: Spawning a machine must complete in <1ms (tokio::spawn overhead).

**P3**: Channel sends must not block unless the channel is full (bounded channels with configurable size).

**P4**: Supervision restart logic must complete within 100ms (detect failure → restart child).

---

## Feature 1: Channel Declarations

### Requirements

**R1.1**: Add `channel` keyword to grammar for declaring typed message channels between machines.

**R1.2**: Channels are **one-to-many broadcast** by default (any machine can send, multiple machines can receive).

**R1.3**: Each channel has a typed message payload (enum or struct).

**R1.4**: Machines declare channel relationships with `sends <Channel>` and `receives <Channel>` annotations.

**R1.5**: Generated Rust code creates `tokio::sync::broadcast` channels for one-to-many, `tokio::sync::mpsc` for one-to-one (if only one receiver).

**R1.6**: Generated Go code creates Go channels with goroutine-based fanout for broadcast.

**R1.7**: Sending to a channel is a new statement: `send <Channel>(<message>);`

**R1.8**: Receiving from a channel is a new effect: `receive <Channel>()` that blocks until a message arrives.

### Grammar Changes

Add to `grammar.pest`:

```pest
program = { SOI ~ (use_decl | type_decl | enum_decl | channel_decl | machine_decl)* ~ EOI }

// Channel declarations
channel_decl = { "channel" ~ ident ~ ":" ~ type_expr ~ ("(" ~ channel_config ~ ")")? }
channel_config = { channel_config_item ~ ("," ~ channel_config_item)* }
channel_config_item = {
    | "capacity" ~ ":" ~ int_lit
    | "mode" ~ ":" ~ channel_mode
}
channel_mode = { "broadcast" | "mpsc" }

// Machine annotations for channels
machine_decl = { "machine" ~ ident ~ machine_annotations? ~ "{" ~ machine_body ~ "}" }
machine_annotations = { "(" ~ machine_annotation ~ ("," ~ machine_annotation)* ~ ")" }
machine_annotation = {
    | "sends" ~ ident
    | "receives" ~ ident
}

// Send statement
send_stmt = { "send" ~ ident ~ "(" ~ expr ~ ")" ~ ";" }
statement = { let_stmt | return_stmt | if_stmt | match_stmt | transition_stmt | effect_stmt | send_stmt | expr_stmt }
```

### AST Changes

Add to `ast.rs`:

```rust
pub struct Program {
    pub uses: Vec<UsePath>,
    pub types: Vec<TypeDecl>,
    pub channels: Vec<ChannelDecl>,  // NEW
    pub machines: Vec<MachineDecl>,
}

pub struct ChannelDecl {
    pub name: String,
    pub message_type: TypeExpr,
    pub capacity: Option<i64>,      // Buffer size (None = unbounded)
    pub mode: ChannelMode,           // broadcast or mpsc
}

pub enum ChannelMode {
    Broadcast,  // One-to-many (tokio::sync::broadcast)
    Mpsc,       // One-to-one (tokio::sync::mpsc)
}

pub struct MachineDecl {
    pub name: String,
    pub sends: Vec<String>,         // NEW: Channel names this machine sends to
    pub receives: Vec<String>,      // NEW: Channel names this machine receives from
    pub states: Vec<StateDecl>,
    pub transitions: Vec<TransitionDecl>,
    pub handlers: Vec<OnHandler>,
    pub effects: Vec<EffectDecl>,
}

pub enum Statement {
    Let { name: String, ty: Option<TypeExpr>, value: Expr },
    Return(Expr),
    If { condition: Expr, then_block: Block, else_block: Option<Block> },
    Match { scrutinee: Expr, arms: Vec<MatchArm> },
    Goto { state: String, args: Vec<Expr> },
    Perform { effect: String, args: Vec<Expr> },
    Send { channel: String, message: Expr },  // NEW
    Expr(Expr),
}
```

### Acceptance Criteria

**AC1.1**: A `.gu` file with `channel OrderEvents: OrderEvent` generates Rust code with a `tokio::sync::broadcast` channel type.

**AC1.2**: A machine with `(sends OrderEvents)` generates a `send_order_events(&self, msg: OrderEvent)` method.

**AC1.3**: A machine with `(receives OrderEvents)` generates a `receive_order_events(&mut self) -> OrderEvent` async method.

**AC1.4**: The `send` statement in a handler body generates a channel send operation.

**AC1.5**: Channel capacity configuration translates to bounded channel size in Rust/Go.

**AC1.6**: Multiple machines can receive from the same broadcast channel.

### Test Cases

**TC1.1 - Basic Channel Declaration**

Input:
```gust
use crate::types::OrderEvent;

channel OrderEvents: OrderEvent (capacity: 100, mode: broadcast)

machine OrderProcessor (sends OrderEvents) {
    state Processing(order_id: String)
    state Done

    transition complete: Processing -> Done

    async on complete(ctx: Context) {
        let event = OrderEvent::Completed(order_id.clone());
        send OrderEvents(event);
        goto Done();
    }
}

machine OrderNotifier (receives OrderEvents) {
    state Listening

    effect handle_order_event(event: OrderEvent) -> ()

    transition on_event: Listening -> Listening

    async on on_event(ctx: Context) {
        let event = receive OrderEvents();
        perform handle_order_event(event);
        goto Listening();
    }
}
```

Expected Rust output (OrderProcessor):

```rust
// Generated channel infrastructure
pub struct OrderEventsChannel {
    sender: tokio::sync::broadcast::Sender<OrderEvent>,
}

impl OrderEventsChannel {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = tokio::sync::broadcast::channel(capacity);
        Self { sender }
    }

    pub fn sender(&self) -> tokio::sync::broadcast::Sender<OrderEvent> {
        self.sender.clone()
    }

    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<OrderEvent> {
        self.sender.subscribe()
    }
}

// OrderProcessor machine
impl OrderProcessor {
    pub async fn complete(
        &mut self,
        ctx: Context,
        order_events_tx: &tokio::sync::broadcast::Sender<OrderEvent>,
    ) -> Result<(), OrderProcessorError> {
        match &self.state {
            OrderProcessorState::Processing { order_id } => {
                let event = OrderEvent::Completed(order_id.clone());
                let _ = order_events_tx.send(event); // Ignore send errors (no receivers is ok)
                self.state = OrderProcessorState::Done;
                Ok(())
            }
            _ => Err(OrderProcessorError::InvalidTransition {
                transition: "complete".to_string(),
                from: format!("{:?}", self.state),
            }),
        }
    }
}
```

Expected Rust output (OrderNotifier):

```rust
impl OrderNotifier {
    pub async fn on_event(
        &mut self,
        ctx: Context,
        order_events_rx: &mut tokio::sync::broadcast::Receiver<OrderEvent>,
        effects: &impl OrderNotifierEffects,
    ) -> Result<(), OrderNotifierError> {
        match &self.state {
            OrderNotifierState::Listening => {
                let event = order_events_rx.recv().await
                    .map_err(|e| OrderNotifierError::Failed {
                        reason: format!("channel closed: {}", e)
                    })?;
                effects.handle_order_event(&event).await;
                self.state = OrderNotifierState::Listening;
                Ok(())
            }
            _ => Err(OrderNotifierError::InvalidTransition {
                transition: "on_event".to_string(),
                from: format!("{:?}", self.state),
            }),
        }
    }
}
```

**TC1.2 - MPSC Channel (One-to-One)**

Input:
```gust
channel Commands: CommandRequest (mode: mpsc, capacity: 10)

machine CommandIssuer (sends Commands) {
    state Ready
    // ...
}

machine CommandExecutor (receives Commands) {
    state Idle
    // ...
}
```

Expected: Generates `tokio::sync::mpsc::channel` instead of broadcast.

**TC1.3 - Unbounded Channel**

Input:
```gust
channel Logs: LogMessage  // No capacity = unbounded
```

Expected: Generates `tokio::sync::mpsc::unbounded_channel()` (or broadcast unbounded).

### Implementation Guide

**Step 1: Update AST types**

File: `D:\Projects\gust\gust-lang\src\ast.rs`

Add channel types at the end of the file:
```rust
pub struct ChannelDecl {
    pub name: String,
    pub message_type: TypeExpr,
    pub capacity: Option<i64>,
    pub mode: ChannelMode,
}

pub enum ChannelMode {
    Broadcast,
    Mpsc,
}

impl Default for ChannelMode {
    fn default() -> Self {
        ChannelMode::Broadcast
    }
}
```

Update `Program`:
```rust
pub struct Program {
    pub uses: Vec<UsePath>,
    pub types: Vec<TypeDecl>,
    pub channels: Vec<ChannelDecl>,  // ADD THIS
    pub machines: Vec<MachineDecl>,
}
```

Update `MachineDecl`:
```rust
pub struct MachineDecl {
    pub name: String,
    pub sends: Vec<String>,      // ADD THIS
    pub receives: Vec<String>,   // ADD THIS
    pub states: Vec<StateDecl>,
    pub transitions: Vec<TransitionDecl>,
    pub handlers: Vec<OnHandler>,
    pub effects: Vec<EffectDecl>,
}
```

Update `Statement` enum:
```rust
pub enum Statement {
    // ... existing variants ...
    Send { channel: String, message: Expr },  // ADD THIS before Expr variant
    Expr(Expr),
}
```

**Step 2: Update grammar**

File: `D:\Projects\gust\gust-lang\src\grammar.pest`

Insert after line 17 (`field       = { ident ~ ":" ~ type_expr }`):
```pest
// === Channel Declarations ===
channel_decl = { "channel" ~ ident ~ ":" ~ type_expr ~ channel_config? }
channel_config = { "(" ~ channel_config_item ~ ("," ~ channel_config_item)* ~ ")" }
channel_config_item = {
    ("capacity" ~ ":" ~ int_lit) |
    ("mode" ~ ":" ~ channel_mode)
}
channel_mode = { "broadcast" | "mpsc" }
```

Update line 8:
```pest
program = { SOI ~ (use_decl | type_decl | channel_decl | machine_decl)* ~ EOI }
```

Update line 25:
```pest
machine_decl = { "machine" ~ ident ~ machine_annotations? ~ "{" ~ machine_body ~ "}" }
machine_annotations = { "(" ~ machine_annotation ~ ("," ~ machine_annotation)* ~ ")" }
machine_annotation = { ("sends" ~ ident) | ("receives" ~ ident) }
```

Insert after line 51 (effect_stmt):
```pest
send_stmt = { "send" ~ ident ~ "(" ~ expr ~ ")" ~ ";" }
```

Update line 46:
```pest
statement = { let_stmt | return_stmt | if_stmt | transition_stmt | effect_stmt | send_stmt | expr_stmt }
```

**Step 3: Implement Rust codegen for channels**

File: `D:\Projects\gust\gust-lang\src\codegen.rs`

Add after `emit_type_decl()` method:
```rust
fn emit_channel_types(&mut self, program: &Program) {
    for channel in &program.channels {
        self.emit_channel_struct(channel);
        self.newline();
    }
}

fn emit_channel_struct(&mut self, channel: &ChannelDecl) {
    let name = &channel.name;
    let msg_type = self.type_expr_to_rust(&channel.message_type);

    match channel.mode {
        ChannelMode::Broadcast => {
            self.line(&format!("pub struct {name}Channel {{"));
            self.indent += 1;
            self.line(&format!("sender: tokio::sync::broadcast::Sender<{msg_type}>,"));
            self.indent -= 1;
            self.line("}");
            self.newline();

            self.line(&format!("impl {name}Channel {{"));
            self.indent += 1;

            // Constructor
            if let Some(cap) = channel.capacity {
                self.line(&format!("pub fn new() -> Self {{"));
                self.indent += 1;
                self.line(&format!("let (sender, _) = tokio::sync::broadcast::channel({cap});"));
            } else {
                self.line(&format!("pub fn new(capacity: usize) -> Self {{"));
                self.indent += 1;
                self.line("let (sender, _) = tokio::sync::broadcast::channel(capacity);");
            }
            self.line("Self { sender }");
            self.indent -= 1;
            self.line("}");
            self.newline();

            // Sender accessor
            self.line(&format!("pub fn sender(&self) -> tokio::sync::broadcast::Sender<{msg_type}> {{"));
            self.indent += 1;
            self.line("self.sender.clone()");
            self.indent -= 1;
            self.line("}");
            self.newline();

            // Subscribe method
            self.line(&format!("pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<{msg_type}> {{"));
            self.indent += 1;
            self.line("self.sender.subscribe()");
            self.indent -= 1;
            self.line("}");

            self.indent -= 1;
            self.line("}");
        }
        ChannelMode::Mpsc => {
            // Similar but using tokio::sync::mpsc
            self.line("// MPSC channel - use tokio::sync::mpsc directly");
        }
    }
}
```

Update `generate()` to call emit_channel_types:
```rust
pub fn generate(mut self, program: &Program) -> String {
    self.emit_prelude(program);

    // Emit type declarations as Rust structs
    for type_decl in &program.types {
        self.emit_type_decl(type_decl);
        self.newline();
    }

    // Emit channel infrastructure
    self.emit_channel_types(program);  // ADD THIS

    // Emit each machine
    for machine in &program.machines {
        self.emit_machine(machine);
        self.newline();
    }

    self.output
}
```

Update transition methods to accept channel senders/receivers. Modify `emit_transition_method()` to check if the machine sends/receives channels and add them as parameters.

**Step 4: Update statement codegen for send**

In `emit_statement()`, add the Send case:
```rust
Statement::Send { channel, message } => {
    let msg_expr = self.expr_to_rust(message);
    // Assume channel sender is passed as parameter named {channel_name}_tx
    self.line(&format!("let _ = {}_tx.send({});",
        snake_case(channel), msg_expr));
}
```

Add helper function:
```rust
fn snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(c.to_ascii_lowercase());
    }
    result
}
```

---

## Feature 2: Supervision Trees

### Requirements

**R2.1**: Add `supervises` keyword to declare parent-child relationships between machines.

**R2.2**: Supervisor machines can specify restart strategies: `one_for_one`, `one_for_all`, `rest_for_one`.

**R2.3**: When a child machine transitions to an error state or returns an error from a transition, the supervisor's `on_child_failure` callback is invoked.

**R2.4**: The supervisor can decide to: Restart the child (create new instance in initial state), Escalate the error up the supervision tree, or Ignore the failure.

**R2.5**: Generated Rust code uses `tokio::task::JoinSet` to manage child tasks, with automatic restart logic.

**R2.6**: Generated Go code uses `golang.org/x/sync/errgroup` for supervision, with goroutine-based restart.

**R2.7**: Supervisors track child machine instances by ID (string identifier).

**R2.8**: Child machines can be spawned with `spawn` keyword in handler bodies.

### Grammar Changes

```pest
// Machine supervision declaration
machine_decl = { "machine" ~ ident ~ machine_config? ~ "{" ~ machine_body ~ "}" }

machine_config = { "(" ~ machine_config_item ~ ("," ~ machine_config_item)* ~ ")" }
machine_config_item = {
    | "sends" ~ ident
    | "receives" ~ ident
    | "supervises" ~ ident ~ ("(" ~ supervision_strategy ~ ")")?
}

supervision_strategy = { "one_for_one" | "one_for_all" | "rest_for_one" }

// Spawn statement for creating child machines
spawn_stmt = { "spawn" ~ ident ~ "(" ~ ident ~ "," ~ expr ~ ")" ~ ";" }
// Usage: spawn OrderProcessor(child_id, initial_args);

statement = { let_stmt | return_stmt | if_stmt | match_stmt | transition_stmt | effect_stmt | send_stmt | spawn_stmt | expr_stmt }
```

### AST Changes

```rust
pub struct MachineDecl {
    pub name: String,
    pub sends: Vec<String>,
    pub receives: Vec<String>,
    pub supervises: Vec<SupervisionSpec>,  // NEW
    pub states: Vec<StateDecl>,
    pub transitions: Vec<TransitionDecl>,
    pub handlers: Vec<OnHandler>,
    pub effects: Vec<EffectDecl>,
}

pub struct SupervisionSpec {
    pub child_machine: String,
    pub strategy: SupervisionStrategy,
}

pub enum SupervisionStrategy {
    OneForOne,    // Restart only the failed child
    OneForAll,    // Restart all children if one fails
    RestForOne,   // Restart the failed child and all children started after it
}

impl Default for SupervisionStrategy {
    fn default() -> Self {
        SupervisionStrategy::OneForOne
    }
}

pub enum Statement {
    Let { name: String, ty: Option<TypeExpr>, value: Expr },
    Return(Expr),
    If { condition: Expr, then_block: Block, else_block: Option<Block> },
    Match { scrutinee: Expr, arms: Vec<MatchArm> },
    Goto { state: String, args: Vec<Expr> },
    Perform { effect: String, args: Vec<Expr> },
    Send { channel: String, message: Expr },
    Spawn { machine: String, child_id: String, args: Expr },  // NEW
    Expr(Expr),
}
```

### Acceptance Criteria

**AC2.1**: A machine with `(supervises OrderProcessor)` generates supervisor infrastructure code.

**AC2.2**: The `spawn` statement creates a new child machine instance with a unique ID.

**AC2.3**: Child machine errors trigger the supervisor's failure handler.

**AC2.4**: Restart strategies are implemented correctly (one-for-one restarts only the failed child).

**AC2.5**: The supervisor maintains a registry of active children (Map<String, ChildHandle>).

**AC2.6**: Supervisor graceful shutdown terminates all children.

### Test Cases

**TC2.1 - Basic Supervision**

Input:
```gust
use crate::types::{Order, OrderError};

machine OrderProcessor {
    state Processing(order: Order)
    state Done
    state Failed(error: OrderError)

    transition process: Processing -> Done | Failed
    transition retry: Failed -> Processing

    async on process(ctx: Context) {
        // Simulate failure
        if order.items.is_empty() {
            goto Failed(OrderError::EmptyOrder);
        } else {
            goto Done();
        }
    }
}

machine OrderSupervisor (supervises OrderProcessor(one_for_one)) {
    state Running(active_count: i64)
    state ShuttingDown

    transition start_child: Running -> Running
    transition shutdown: Running -> ShuttingDown

    async on start_child(order_id: String, order: Order) {
        spawn OrderProcessor(order_id, Processing(order));
        goto Running(active_count + 1);
    }

    async on shutdown(ctx: Context) {
        goto ShuttingDown();
    }
}
```

Expected Rust output for supervisor:

```rust
use tokio::task::JoinSet;
use std::collections::HashMap;

pub struct OrderSupervisorRuntime {
    supervisor: OrderSupervisor,
    children: HashMap<String, tokio::task::JoinHandle<Result<(), OrderProcessorError>>>,
}

impl OrderSupervisorRuntime {
    pub fn new() -> Self {
        Self {
            supervisor: OrderSupervisor::new(0),
            children: HashMap::new(),
        }
    }

    pub async fn spawn_child(&mut self, child_id: String, mut machine: OrderProcessor) {
        let handle = tokio::spawn(async move {
            // Run the child machine until completion
            machine.process(Context::default()).await
        });

        self.children.insert(child_id.clone(), handle);
    }

    pub async fn supervise(mut self) -> Result<(), OrderSupervisorError> {
        loop {
            // Monitor children for failures
            let mut failed_children = Vec::new();

            for (child_id, handle) in &mut self.children {
                if handle.is_finished() {
                    match handle.await {
                        Ok(Ok(())) => {
                            // Child completed successfully
                        }
                        Ok(Err(e)) => {
                            // Child failed - invoke supervisor callback
                            failed_children.push((child_id.clone(), e));
                        }
                        Err(e) => {
                            // Task panicked
                            eprintln!("Child {} panicked: {:?}", child_id, e);
                        }
                    }
                }
            }

            // Handle failures according to strategy
            for (child_id, error) in failed_children {
                // one_for_one strategy: restart only the failed child
                self.children.remove(&child_id);
                // TODO: Create new instance and respawn
            }

            // Check for shutdown
            if matches!(self.supervisor.state, OrderSupervisorState::ShuttingDown) {
                break;
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        // Graceful shutdown: wait for all children
        for (_id, handle) in self.children {
            let _ = handle.await;
        }

        Ok(())
    }
}
```

**TC2.2 - one_for_all Strategy**

Input:
```gust
machine Coordinator (supervises Worker(one_for_all)) {
    state Active
    // ...
}
```

Expected: If any Worker fails, all Workers are restarted.

**TC2.3 - rest_for_one Strategy**

Input:
```gust
machine Pipeline (supervises Stage(rest_for_one)) {
    // Stages: A -> B -> C
    // If B fails, restart B and C (but not A)
}
```

Expected: Children started after the failed child are also restarted.

### Implementation Guide

**Step 1: Update AST for supervision**

Add to `ast.rs`:
```rust
pub struct SupervisionSpec {
    pub child_machine: String,
    pub strategy: SupervisionStrategy,
}

pub enum SupervisionStrategy {
    OneForOne,
    OneForAll,
    RestForOne,
}
```

Update `Statement` enum with Spawn variant.

**Step 2: Update runtime library**

File: `D:\Projects\gust\gust-runtime\src\lib.rs`

Add supervision runtime infrastructure:
```rust
use tokio::task::JoinSet;
use std::collections::HashMap;
use std::sync::Arc;

pub struct SupervisorRuntime<M: Machine, C: Machine> {
    pub supervisor: M,
    pub children: HashMap<String, ChildHandle<C>>,
    pub strategy: SupervisionStrategy,
}

pub struct ChildHandle<C: Machine> {
    pub task: tokio::task::JoinHandle<Result<(), C::Error>>,
    pub spawn_order: usize,  // For rest-for-one strategy
}

pub enum SupervisionStrategy {
    OneForOne,
    OneForAll,
    RestForOne,
}

impl<M: Machine, C: Machine> SupervisorRuntime<M, C> {
    pub fn new(supervisor: M, strategy: SupervisionStrategy) -> Self {
        Self {
            supervisor,
            children: HashMap::new(),
            strategy,
        }
    }

    pub fn spawn_child<F>(&mut self, child_id: String, init: F)
    where
        F: FnOnce() -> C + Send + 'static,
    {
        let spawn_order = self.children.len();
        let child = init();

        let handle = tokio::spawn(async move {
            // Run child machine
            // This is a simplified example - real implementation
            // would need machine-specific run loop
            Ok(())
        });

        self.children.insert(child_id, ChildHandle {
            task: handle,
            spawn_order,
        });
    }

    pub async fn monitor(&mut self) -> Result<(), M::Error> {
        loop {
            // Check for completed/failed children
            let mut to_remove = Vec::new();
            let mut to_restart = Vec::new();

            for (id, handle) in &mut self.children {
                if handle.task.is_finished() {
                    to_remove.push(id.clone());

                    // Determine restart strategy
                    match self.strategy {
                        SupervisionStrategy::OneForOne => {
                            to_restart.push(id.clone());
                        }
                        SupervisionStrategy::OneForAll => {
                            // Restart all children
                            to_restart.extend(self.children.keys().cloned());
                            break;
                        }
                        SupervisionStrategy::RestForOne => {
                            // Restart this child and all started after it
                            let failed_order = handle.spawn_order;
                            for (other_id, other_handle) in &self.children {
                                if other_handle.spawn_order >= failed_order {
                                    to_restart.push(other_id.clone());
                                }
                            }
                        }
                    }
                }
            }

            // Remove failed children
            for id in &to_remove {
                self.children.remove(id);
            }

            // Restart children
            for id in to_restart {
                // TODO: Restart logic - need to recreate child machines
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }
}
```

**Step 3: Code generation for supervised machines**

In `codegen.rs`, add method to generate supervisor runtime:
```rust
fn emit_supervisor_runtime(&mut self, machine: &MachineDecl, program: &Program) {
    if machine.supervises.is_empty() {
        return;  // Not a supervisor
    }

    for sup_spec in &machine.supervises {
        let child_machine = &sup_spec.child_machine;
        let runtime_name = format!("{}Runtime", machine.name);

        self.line(&format!("pub struct {runtime_name} {{"));
        self.indent += 1;
        self.line(&format!("supervisor: {},", machine.name));
        self.line(&format!(
            "children: HashMap<String, tokio::task::JoinHandle<Result<(), {}Error>>>,",
            child_machine
        ));
        self.indent -= 1;
        self.line("}");
        self.newline();

        // Impl block with spawn and monitor methods
        self.line(&format!("impl {runtime_name} {{"));
        self.indent += 1;

        // Constructor
        self.line("pub fn new(supervisor: {}) -> Self {{", machine.name);
        self.indent += 1;
        self.line("Self {");
        self.indent += 1;
        self.line("supervisor,");
        self.line("children: HashMap::new(),");
        self.indent -= 1;
        self.line("}");
        self.indent -= 1;
        self.line("}");
        self.newline();

        // spawn_child method
        // monitor method
        // shutdown method

        self.indent -= 1;
        self.line("}");
    }
}
```

Call `emit_supervisor_runtime` in `emit_machine` when the machine has supervision specs.

---

## Feature 3: Lifecycle Management

### Requirements

**R3.1**: `spawn` creates a child machine and starts it running as a tokio task (Rust) or goroutine (Go).

**R3.2**: Machines can define a `shutdown` transition that's called during graceful termination.

**R3.3**: Timeout support for transitions: `transition process(timeout: 5s): Pending -> Done | Timeout`

**R3.4**: Cancellation tokens flow through supervision trees - when supervisor shuts down, all children receive cancellation.

**R3.5**: Generated code includes `run()` method that executes the machine's event loop until shutdown.

**R3.6**: Rust code uses `tokio::select!` to race transition execution against cancellation.

**R3.7**: Go code uses `context.Context` for cancellation propagation.

### Grammar Changes

```pest
// Timeout on transitions
transition_decl = { "transition" ~ ident ~ timeout_spec? ~ ":" ~ ident ~ "->" ~ target_states }
timeout_spec = { "(" ~ "timeout" ~ ":" ~ duration ~ ")" }
duration = { int_lit ~ time_unit }
time_unit = { "ms" | "s" | "m" | "h" }
```

### AST Changes

```rust
pub struct TransitionDecl {
    pub name: String,
    pub from: String,
    pub targets: Vec<String>,
    pub timeout: Option<Duration>,  // NEW
}

pub struct Duration {
    pub value: i64,
    pub unit: TimeUnit,
}

pub enum TimeUnit {
    Milliseconds,
    Seconds,
    Minutes,
    Hours,
}
```

### Acceptance Criteria

**AC3.1**: Spawned machines run concurrently in tokio tasks.

**AC3.2**: Shutdown signals propagate from supervisor to children.

**AC3.3**: Transitions with timeout specifications race against time limits.

**AC3.4**: Timeout errors are handled gracefully (transition to timeout state or return error).

**AC3.5**: The `run()` method processes transitions in a loop until termination.

### Test Cases

**TC3.1 - Machine Run Loop**

Input:
```gust
machine Worker {
    state Idle
    state Working(task_id: String)
    state Done

    transition start: Idle -> Working
    transition finish: Working -> Done

    async on start(task_id: String) {
        goto Working(task_id);
    }

    async on finish(ctx: Context) {
        goto Done();
    }
}
```

Expected Rust output:
```rust
impl Worker {
    pub async fn run(
        mut self,
        mut shutdown: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<(), WorkerError> {
        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    // Graceful shutdown
                    break;
                }
                // Wait for external events to trigger transitions
                // (In a real system, this would be event channels)
            }
        }
        Ok(())
    }
}
```

**TC3.2 - Transition Timeout**

Input:
```gust
machine SlowProcessor {
    state Processing
    state Done
    state Timeout

    transition process(timeout: 5s): Processing -> Done | Timeout

    async on process(ctx: Context) {
        // Simulate slow operation
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        goto Done();
    }
}
```

Expected Rust output:
```rust
impl SlowProcessor {
    pub async fn process(&mut self, ctx: Context) -> Result<(), SlowProcessorError> {
        match &self.state {
            SlowProcessorState::Processing => {
                let timeout = tokio::time::Duration::from_secs(5);

                tokio::select! {
                    result = async {
                        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                        self.state = SlowProcessorState::Done;
                        Ok(())
                    } => result,
                    _ = tokio::time::sleep(timeout) => {
                        self.state = SlowProcessorState::Timeout;
                        Err(SlowProcessorError::Failed {
                            reason: "transition timeout".to_string()
                        })
                    }
                }
            }
            _ => Err(SlowProcessorError::InvalidTransition {
                transition: "process".to_string(),
                from: format!("{:?}", self.state),
            }),
        }
    }
}
```

**TC3.3 - Graceful Shutdown with Cancellation**

Input:
```gust
machine Service (supervises Worker) {
    state Running
    state ShuttingDown
    state Stopped

    transition shutdown: Running -> ShuttingDown
    transition stopped: ShuttingDown -> Stopped

    async on shutdown(ctx: Context) {
        // Signal all children to shut down
        goto ShuttingDown();
    }
}
```

Expected: Supervisor sends shutdown signal to all children, waits for them to finish, then transitions to Stopped.

### Implementation Guide

**Step 1: Add timeout to AST**

Update `TransitionDecl` in `ast.rs` to include optional timeout field.

**Step 2: Add timeout parsing**

Update `parse_transition_decl()` in `parser.rs` to parse timeout specifications.

**Step 3: Generate run() method**

Add to `codegen.rs`:
```rust
fn emit_run_method(&mut self, machine: &MachineDecl) {
    let error_type = format!("{}Error", machine.name);

    self.line(&format!(
        "pub async fn run(mut self, mut shutdown_rx: tokio::sync::broadcast::Receiver<()>) -> Result<(), {error_type}> {{"
    ));
    self.indent += 1;

    self.line("loop {");
    self.indent += 1;

    self.line("tokio::select! {");
    self.indent += 1;

    self.line("_ = shutdown_rx.recv() => {");
    self.indent += 1;
    self.line("// Graceful shutdown");
    self.line("break;");
    self.indent -= 1;
    self.line("}");

    self.line("// Transition event handling would go here");
    self.line("// In a real implementation, this would receive from event channels");

    self.indent -= 1;
    self.line("}");

    self.indent -= 1;
    self.line("}");

    self.newline();
    self.line("Ok(())");

    self.indent -= 1;
    self.line("}");
}
```

Call this method in `emit_machine()` for all machines.

**Step 4: Wrap transitions with timeout**

Update `emit_transition_method()` to wrap the handler body in `tokio::select!` when timeout is specified:
```rust
if let Some(ref timeout) = transition.timeout {
    let timeout_ms = match timeout.unit {
        TimeUnit::Milliseconds => timeout.value,
        TimeUnit::Seconds => timeout.value * 1000,
        TimeUnit::Minutes => timeout.value * 60000,
        TimeUnit::Hours => timeout.value * 3600000,
    };

    self.line(&format!(
        "let timeout = tokio::time::Duration::from_millis({timeout_ms});"
    ));
    self.newline();

    self.line("tokio::select! {");
    self.indent += 1;

    self.line("result = async {");
    self.indent += 1;
    // Emit handler body here
    self.line("Ok(())");
    self.indent -= 1;
    self.line("} => result,");

    self.line("_ = tokio::time::sleep(timeout) => {");
    self.indent += 1;
    self.line(&format!(
        "Err({error_type}::Failed {{ reason: \"transition timeout\".to_string() }})"
    ));
    self.indent -= 1;
    self.line("}");

    self.indent -= 1;
    self.line("}");
}
```

---

## Feature 4: Cross-Boundary Serialization

### Requirements

**R4.1**: Machines can run in-process (direct function calls) or out-of-process (serialized messages).

**R4.2**: When machines run in different processes, channel messages are automatically serialized via serde (JSON or bincode).

**R4.3**: Generate `.proto` files for each channel type to enable gRPC communication between processes.

**R4.4**: The same `.gu` code generates both in-process and cross-process variants.

**R4.5**: Deployment mode is chosen at runtime via configuration, not compile time.

**R4.6**: Support three deployment modes:
- **Local**: All machines in same process, zero-copy message passing
- **Distributed-JSON**: Machines in different processes, JSON over HTTP
- **Distributed-Protobuf**: Machines in different processes, Protobuf over gRPC

**R4.7**: Generated code includes transport adapters that handle serialization transparently.

### Grammar Changes

No grammar changes needed - this is purely a code generation concern.

### AST Changes

No AST changes needed.

### Acceptance Criteria

**AC4.1**: Generated code compiles in both local and distributed modes.

**AC4.2**: Messages sent over channels are automatically serialized when crossing process boundaries.

**AC4.3**: `.proto` files are generated for all channel message types.

**AC4.4**: Transport adapters handle connection management, retries, and error handling.

**AC4.5**: Performance: Local mode has zero serialization overhead.

**AC4.6**: Performance: Distributed mode uses efficient binary encoding (protobuf or bincode).

### Test Cases

**TC4.1 - Local Mode (In-Process)**

Input:
```gust
channel OrderEvents: OrderEvent
machine OrderProcessor (sends OrderEvents) { /* ... */ }
machine OrderNotifier (receives OrderEvents) { /* ... */ }
```

Runtime configuration:
```toml
[deployment]
mode = "local"
```

Expected: Messages passed via `Arc<OrderEvent>` (zero-copy).

**TC4.2 - Distributed Mode (JSON over HTTP)**

Runtime configuration:
```toml
[deployment]
mode = "distributed-json"

[machines.OrderProcessor]
address = "http://localhost:3001"

[machines.OrderNotifier]
address = "http://localhost:3002"
```

Expected: OrderProcessor sends HTTP POST to OrderNotifier with JSON payload.

**TC4.3 - Distributed Mode (Protobuf over gRPC)**

Runtime configuration:
```toml
[deployment]
mode = "distributed-protobuf"

[machines.OrderProcessor]
address = "localhost:50051"

[machines.OrderNotifier]
address = "localhost:50052"
```

Expected: Generated `.proto` file:
```protobuf
syntax = "proto3";

package gust.orderevents;

message OrderEvent {
  oneof event {
    OrderValidated validated = 1;
    OrderCharged charged = 2;
    OrderShipped shipped = 3;
  }
}

message OrderValidated {
  string order_id = 1;
  Money total = 2;
}

message Money {
  int64 cents = 1;
  string currency = 2;
}

service OrderEventsChannel {
  rpc Send(OrderEvent) returns (google.protobuf.Empty);
  rpc Subscribe(google.protobuf.Empty) returns (stream OrderEvent);
}
```

**TC4.4 - Mixed Deployment**

Configuration:
```toml
[deployment]
default_mode = "local"

[machines.OrderProcessor]
mode = "local"  # Runs in-process

[machines.OrderNotifier]
mode = "distributed-json"  # Runs remotely
address = "http://notification-service:8080"
```

Expected: OrderProcessor runs locally, sends to OrderNotifier over HTTP.

### Implementation Guide

**Step 1: Generate transport adapters**

Add to `codegen.rs`:
```rust
fn emit_channel_transport(&mut self, channel: &ChannelDecl) {
    let name = &channel.name;
    let msg_type = self.type_expr_to_rust(&channel.message_type);

    // Local transport (Arc wrapper for zero-copy)
    self.line(&format!("pub mod {}_local {{", snake_case(name)));
    self.indent += 1;
    self.line("use super::*;");
    self.line("use std::sync::Arc;");
    self.newline();

    self.line(&format!("pub type Message = Arc<{msg_type}>;"));
    self.newline();

    self.line("pub fn send(tx: &tokio::sync::broadcast::Sender<Message>, msg: {msg_type}) {{", msg_type = msg_type);
    self.indent += 1;
    self.line("let _ = tx.send(Arc::new(msg));");
    self.indent -= 1;
    self.line("}");

    self.indent -= 1;
    self.line("}");
    self.newline();

    // Distributed transport (JSON over HTTP)
    self.line(&format!("pub mod {}_distributed_json {{", snake_case(name)));
    self.indent += 1;
    self.line("use super::*;");
    self.line("use serde_json;");
    self.newline();

    self.line(&format!("pub async fn send(url: &str, msg: &{msg_type}) -> Result<(), Box<dyn std::error::Error>> {{"));
    self.indent += 1;
    self.line("let client = reqwest::Client::new();");
    self.line("let response = client");
    self.indent += 1;
    self.line(".post(url)");
    self.line(".json(msg)");
    self.line(".send()");
    self.line(".await?;");
    self.indent -= 1;
    self.line("response.error_for_status()?;");
    self.line("Ok(())");
    self.indent -= 1;
    self.line("}");

    self.indent -= 1;
    self.line("}");
}
```

**Step 2: Generate protobuf definitions**

Add function to generate `.proto` files:
```rust
fn generate_proto_file(channel: &ChannelDecl, program: &Program) -> String {
    let mut output = String::new();
    output.push_str("syntax = \"proto3\";\n\n");
    output.push_str(&format!("package gust.{};\n\n", snake_case(&channel.name)));

    // Generate message definitions
    // This requires traversing the type tree and converting Rust types to protobuf

    output
}
```

Call this in `generate()` for each channel, write to `{channel_name}.proto` file.

**Step 3: Runtime configuration loading**

Add to `gust-runtime`:
```rust
#[derive(Debug, Deserialize)]
pub struct DeploymentConfig {
    pub mode: DeploymentMode,
    pub machines: HashMap<String, MachineConfig>,
}

#[derive(Debug, Deserialize)]
pub enum DeploymentMode {
    Local,
    DistributedJson,
    DistributedProtobuf,
}

#[derive(Debug, Deserialize)]
pub struct MachineConfig {
    pub mode: Option<DeploymentMode>,
    pub address: Option<String>,
}

pub fn load_deployment_config(path: &str) -> Result<DeploymentConfig, Box<dyn std::error::Error>> {
    let contents = std::fs::read_to_string(path)?;
    let config: DeploymentConfig = toml::from_str(&contents)?;
    Ok(config)
}
```

---

## Complete Example: E-Commerce Order Processing System

This example demonstrates all Phase 3 features working together.

### Source Code

File: `examples/ecommerce.gu`

```gust
use crate::types::{Order, Money, Receipt, OrderEvent, NotificationEvent};

// ===== Channel Declarations =====

channel OrderEvents: OrderEvent (capacity: 100, mode: broadcast)
channel NotificationEvents: NotificationEvent (capacity: 50)

// ===== Type Definitions =====

enum OrderEvent {
    Received(Order),
    Validated(Order, Money),
    Charged(Order, Receipt),
    Shipped(Order, String),  // tracking number
    Failed(String),  // reason
}

enum NotificationEvent {
    SendEmail(String, String),  // recipient, body
    SendSMS(String, String),
}

// ===== Machines =====

machine OrderProcessor (sends OrderEvents, receives NotificationEvents) {
    state Pending(order: Order)
    state Validated(order: Order, total: Money)
    state Charged(order: Order, receipt: Receipt)
    state Shipped(order: Order, tracking: String)
    state Failed(reason: String)

    async effect validate_order(order: Order) -> Result<Money, String>
    async effect charge_payment(total: Money) -> Result<Receipt, String>
    async effect create_shipment(order: Order) -> Result<String, String>

    transition validate(timeout: 10s): Pending -> Validated | Failed
    transition charge(timeout: 30s): Validated -> Charged | Failed
    transition ship: Charged -> Shipped | Failed

    async on validate(ctx: Context) {
        send OrderEvents(OrderEvent::Received(order.clone()));

        match perform validate_order(order.clone()) {
            Ok(total) => {
                send OrderEvents(OrderEvent::Validated(order.clone(), total.clone()));
                goto Validated(order, total);
            }
            Err(reason) => {
                send OrderEvents(OrderEvent::Failed(reason.clone()));
                goto Failed(reason);
            }
        }
    }

    async on charge(ctx: Context) {
        match perform charge_payment(total.clone()) {
            Ok(receipt) => {
                send OrderEvents(OrderEvent::Charged(order.clone(), receipt.clone()));
                goto Charged(order, receipt);
            }
            Err(reason) => {
                send OrderEvents(OrderEvent::Failed(reason.clone()));
                goto Failed(reason);
            }
        }
    }

    async on ship(ctx: Context) {
        match perform create_shipment(order.clone()) {
            Ok(tracking) => {
                send OrderEvents(OrderEvent::Shipped(order.clone(), tracking.clone()));
                send NotificationEvents(NotificationEvent::SendEmail(
                    order.customer.clone(),
                    format!("Your order has shipped! Tracking: {}", tracking)
                ));
                goto Shipped(order, tracking);
            }
            Err(reason) => {
                goto Failed(reason);
            }
        }
    }
}

machine NotificationService (receives NotificationEvents) {
    state Listening

    async effect send_email(recipient: String, body: String) -> ()
    async effect send_sms(recipient: String, body: String) -> ()

    transition on_event: Listening -> Listening

    async on on_event(ctx: Context) {
        let event = receive NotificationEvents();

        match event {
            NotificationEvent::SendEmail(recipient, body) => {
                perform send_email(recipient, body);
            }
            NotificationEvent::SendSMS(recipient, body) => {
                perform send_sms(recipient, body);
            }
        }

        goto Listening();
    }
}

machine OrderOrchestrator (
    supervises OrderProcessor(one_for_one),
    supervises NotificationService(one_for_one)
) {
    state Running(active_orders: i64)
    state ShuttingDown
    state Stopped

    transition submit_order: Running -> Running
    transition shutdown: Running -> ShuttingDown
    transition stopped: ShuttingDown -> Stopped

    async on submit_order(order_id: String, order: Order) {
        spawn OrderProcessor(order_id, Pending(order));
        goto Running(active_orders + 1);
    }

    async on shutdown(ctx: Context) {
        // Gracefully shutdown all children
        goto ShuttingDown();
    }

    async on stopped(ctx: Context) {
        goto Stopped();
    }
}
```

### Generated Rust Code Structure

```
ecommerce.g.rs
├── Channel types
│   ├── OrderEventsChannel (broadcast)
│   ├── NotificationEventsChannel (mpsc)
│   └── Transport adapters (local, json, protobuf)
├── OrderEvent enum
├── NotificationEvent enum
├── OrderProcessor machine
│   ├── OrderProcessorState enum
│   ├── OrderProcessor struct
│   ├── OrderProcessorEffects trait
│   ├── OrderProcessorError enum
│   ├── Transition methods (validate, charge, ship)
│   └── run() method
├── NotificationService machine
│   ├── NotificationServiceState enum
│   ├── NotificationService struct
│   ├── NotificationServiceEffects trait
│   ├── Transition methods (on_event)
│   └── run() method
└── OrderOrchestrator machine
    ├── OrderOrchestratorState enum
    ├── OrderOrchestrator struct
    ├── OrderOrchestratorRuntime (supervisor infrastructure)
    ├── Transition methods (submit_order, shutdown, stopped)
    └── Supervision logic (spawn, monitor, restart)
```

### Deployment Configuration

File: `deployment.toml`

```toml
[deployment]
default_mode = "local"

[channels.OrderEvents]
mode = "distributed-json"
# All OrderEvents go over HTTP for audit logging

[channels.NotificationEvents]
mode = "local"
# Notifications processed in-process for low latency

[machines.OrderProcessor]
mode = "local"
# Runs in main service process

[machines.NotificationService]
mode = "distributed-json"
address = "http://notification-service:8080"
# Separate service for sending notifications

[machines.OrderOrchestrator]
mode = "local"
# Supervisor runs in main process
```

---

## Verification Checklist

### Feature 1: Channel Declarations

- [ ] Channel declarations parsed correctly from `.gu` files
- [ ] `sends` and `receives` annotations on machines parsed
- [ ] Broadcast channels generate `tokio::sync::broadcast`
- [ ] MPSC channels generate `tokio::sync::mpsc`
- [ ] Channel capacity configuration works (bounded/unbounded)
- [ ] `send` statement generates channel send code
- [ ] Machines receiving from channels get receiver parameters
- [ ] Generated Rust code compiles without warnings
- [ ] Generated Go code compiles and uses Go channels
- [ ] Multiple receivers can subscribe to broadcast channels
- [ ] Test: Send message from one machine, receive in another

### Feature 2: Supervision Trees

- [ ] `supervises` annotation parsed correctly
- [ ] Supervision strategies (one-for-one, one-for-all, rest-for-one) parsed
- [ ] `spawn` statement generates child machine creation code
- [ ] SupervisorRuntime struct generated with child registry
- [ ] one-for-one strategy restarts only failed child
- [ ] one-for-all strategy restarts all children on any failure
- [ ] rest-for-one strategy restarts failed child and later children
- [ ] Supervisor monitors child task completion
- [ ] Child errors propagate to supervisor
- [ ] Supervisor tracks children by ID
- [ ] Graceful shutdown terminates all children
- [ ] Test: Spawn child, cause failure, verify restart

### Feature 3: Lifecycle Management

- [ ] Timeout specifications on transitions parsed
- [ ] Duration parsing (ms, s, m, h) works correctly
- [ ] Generated code wraps timed transitions in `tokio::select!`
- [ ] Timeout errors handled correctly (transition to timeout state)
- [ ] `run()` method generated for all machines
- [ ] Shutdown signal propagates through supervision tree
- [ ] Cancellation tokens work (tokio broadcast channel)
- [ ] Machines respond to shutdown within reasonable time (<1s)
- [ ] Test: Transition with 1s timeout actually times out
- [ ] Test: Shutdown signal stops all running machines

### Feature 4: Cross-Boundary Serialization

- [ ] Transport adapters generated (local, json, protobuf modules)
- [ ] Local mode uses Arc for zero-copy
- [ ] Distributed-JSON mode uses HTTP + serde_json
- [ ] `.proto` files generated for all channel types
- [ ] Protobuf message definitions match Rust types
- [ ] Deployment config loaded from TOML file
- [ ] Runtime selects correct transport based on config
- [ ] Performance: Local mode has no serialization overhead
- [ ] Performance: JSON mode works but slower than local
- [ ] Test: Send message in local mode (verify no serialization)
- [ ] Test: Send message in distributed mode (verify HTTP call)
- [ ] Test: Protobuf encoding/decoding round-trip

### Integration & End-to-End

- [ ] Complete ecommerce example compiles and runs
- [ ] Order submitted → validated → charged → shipped flow works
- [ ] Channels deliver messages between machines
- [ ] Supervisor restarts failed OrderProcessor instances
- [ ] Notifications sent to external service
- [ ] Timeout on payment processing works correctly
- [ ] Graceful shutdown stops all machines cleanly
- [ ] Deployment config switches between local/distributed modes
- [ ] No memory leaks (run for 1000 orders, check memory)
- [ ] No panics or crashes under normal operation
- [ ] Error handling works (invalid orders, payment failures)

### Code Quality

- [ ] Generated code formatted consistently
- [ ] No compiler warnings
- [ ] Clippy passes with no warnings
- [ ] All error messages are helpful and specific
- [ ] Panic messages include context
- [ ] Comments explain non-obvious concurrency patterns
- [ ] Generated code is readable (a human can understand it at 2am)

---

## File Map

### Files to Create

1. **D:\Projects\gust\gust-lang\src\channel.rs** - Channel AST and codegen utilities (300 lines)
2. **D:\Projects\gust\gust-lang\src\supervisor.rs** - Supervision AST and codegen (400 lines)
3. **D:\Projects\gust\gust-runtime\src\supervisor.rs** - Supervisor runtime (500 lines)
4. **D:\Projects\gust\gust-runtime\src\transport.rs** - Transport adapters (400 lines)
5. **D:\Projects\gust\examples\ecommerce.gu** - Complete example (200 lines)
6. **D:\Projects\gust\examples\ecommerce_effects.rs** - Effect implementations for example (150 lines)
7. **D:\Projects\gust\gust-lang\tests\channel_codegen.rs** - Channel tests (200 lines)
8. **D:\Projects\gust\gust-lang\tests\supervision_codegen.rs** - Supervision tests (250 lines)
9. **D:\Projects\gust\docs\examples\deployment.toml** - Deployment config example (50 lines)

### Files to Modify

1. **D:\Projects\gust\gust-lang\src\grammar.pest**
   - Add `channel_decl` rule (line ~18)
   - Add `machine_annotations` rule (line ~25)
   - Add `supervision_strategy` rule (line ~28)
   - Add `send_stmt` rule (line ~52)
   - Add `spawn_stmt` rule (line ~53)
   - Add `timeout_spec` and `duration` rules (line ~34)

2. **D:\Projects\gust\gust-lang\src\ast.rs**
   - Add `ChannelDecl`, `ChannelMode` (line ~150)
   - Add `SupervisionSpec`, `SupervisionStrategy` (line ~160)
   - Add `Duration`, `TimeUnit` (line ~170)
   - Update `Program` to include channels (line ~5)
   - Update `MachineDecl` with sends/receives/supervises (line ~40)
   - Update `TransitionDecl` with timeout (line ~55)
   - Add `Send` and `Spawn` to `Statement` enum (line ~110)

3. **D:\Projects\gust\gust-lang\src\parser.rs**
   - Add `parse_channel_decl()` (line ~55)
   - Add `parse_supervision_spec()` (line ~120)
   - Update `parse_program()` to include channels (line ~15)
   - Update `parse_machine_decl()` to parse annotations (line ~85)
   - Update `parse_transition_decl()` to parse timeout (line ~125)
   - Add `Send` and `Spawn` cases to `parse_statement()` (line ~230)

4. **D:\Projects\gust\gust-lang\src\codegen.rs**
   - Add `emit_channel_types()` method (line ~70)
   - Add `emit_channel_transport()` method (line ~100)
   - Add `emit_supervisor_runtime()` method (line ~150)
   - Add `emit_run_method()` method (line ~200)
   - Update `emit_transition_method()` to wrap timeout (line ~280)
   - Update `emit_statement()` for Send and Spawn (line ~420)
   - Update `generate()` to call new emission methods (line ~30)

5. **D:\Projects\gust\gust-lang\src\codegen_go.rs**
   - Similar changes for Go codegen
   - Use Go channels instead of tokio
   - Use context.Context for cancellation
   - Use errgroup for supervision

6. **D:\Projects\gust\gust-runtime\src\lib.rs**
   - Add `SupervisorRuntime` struct (line ~80)
   - Add `ChildHandle` struct (line ~100)
   - Add `DeploymentConfig` types (line ~120)
   - Add `load_deployment_config()` function (line ~150)

7. **D:\Projects\gust\gust-runtime\Cargo.toml**
   - Add dependencies: `tokio = { version = "1", features = ["full"] }`
   - Add `reqwest = { version = "0.11", features = ["json"] }`
   - Add `toml = "0.8"`
   - Add `prost = "0.12"` (for protobuf)

8. **D:\Projects\gust\Cargo.toml**
   - No changes needed (workspace already configured)

---

## Success Metrics

Phase 3 is complete when:

1. ✅ Two machines can communicate via typed channels
2. ✅ A supervisor can spawn, monitor, and restart child machines
3. ✅ Graceful shutdown propagates through supervision trees
4. ✅ Transition timeouts work correctly
5. ✅ The same `.gu` code runs in local and distributed modes
6. ✅ The ecommerce example runs end-to-end successfully
7. ✅ Generated code has no warnings or clippy errors
8. ✅ Documentation includes complete examples of all features
9. ✅ All tests pass (unit, integration, end-to-end)
10. ✅ Performance: 10,000 messages/second in local mode

**Target date**: 6-8 weeks from start of implementation.

**Estimated LOC**:
- New code: ~2,500 lines
- Modified code: ~1,000 lines
- Tests: ~1,000 lines
- Documentation/Examples: ~500 lines
- **Total**: ~5,000 lines

---

END OF SPEC

