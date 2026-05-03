# Workflow Runtime Integration

This guide is for people building workflow runtimes, durable-execution engines,
or replay-aware processors that consume Gust state-machine contracts. If you are
writing application code that uses a Gust machine inside a Tokio-based service,
see the [Tokio Integration](tokio_integration.md) guide instead.

---

## Contents

1. [The `gust_parse` MCP contract](#the-gust_parse-mcp-contract)
2. [`effect` vs `action` replay semantics](#effect-vs-action-replay-semantics)
3. [`EngineFailure` as a stdlib contract](#enginefailure-as-a-stdlib-contract)
4. [Go codegen pipeline](#go-codegen-pipeline)
5. [References](#references)

---

## The `gust_parse` MCP Contract

Gust ships an MCP (Model Context Protocol) server, `gust-mcp`, that exposes the
compiler as a set of JSON-RPC tools. Workflow runtimes use the `gust_parse` tool
to introspect `.gu` source files without embedding the Rust compiler in their own
build pipeline.

The server speaks JSON-RPC 2.0 over stdin/stdout with Content-Length framing.

### Sending a `gust_parse` request

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "gust_parse",
    "arguments": {
      "file": "/absolute/path/to/machine.gu"
    }
  }
}
```

### Minimal `.gu` input

```gust
machine OrderProcessor {
    state Idle
    state Processing(order_id: String)
    state Done(receipt: String)
    state Faulted(reason: String)

    transition submit:   Idle       -> Processing
    transition complete: Processing -> Done | Faulted
    transition retry:    Faulted    -> Processing

    effect validate_order(order_id: String) -> bool
    action charge_card(order_id: String, amount_cents: i64) -> String

    on submit(order_id: String) {
        goto Processing(order_id);
    }

    on complete(ctx: CompleteCtx) {
        let valid = perform validate_order(ctx.order_id);
        if valid {
            let receipt = perform charge_card(ctx.order_id, 1000);
            goto Done(receipt);
        } else {
            goto Faulted("validation failed");
        }
    }

    on retry(ctx: FaultedCtx, order_id: String) {
        goto Processing(order_id);
    }
}
```

### `gust_parse` JSON output shape

`gust_parse` returns the full AST as JSON inside a standard MCP content block:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "<JSON string below>"
      }
    ]
  }
}
```

The `text` value is a pretty-printed JSON object with four top-level keys:

```json
{
  "uses": [],
  "types": [],
  "channels": [],
  "machines": [ ... ]
}
```

#### `types[]`

Each entry is either a struct or an enum. **Enum variant payloads are positional**
— there are no named fields in variant positions, only an ordered list of types.
When a `.gu` file declares a type like:

```gust
enum OrderStatus {
    Pending,
    Filled(String),
    Failed(String, i64),
}
```

the JSON representation is:

```json
{
  "kind": "enum",
  "name": "OrderStatus",
  "variants": [
    {
      "name": "Pending",
      "payload": []
    },
    {
      "name": "Filled",
      "payload": ["String"]
    },
    {
      "name": "Failed",
      "payload": ["String", "int64"]
    }
  ]
}
```

A struct entry looks like:

```json
{
  "kind": "struct",
  "name": "OrderConfig",
  "fields": [
    { "name": "max_retries", "type": "int64" },
    { "name": "timeout_ms",  "type": "int64" }
  ]
}
```

Generic types are represented as `{ "generic": "Vec", "args": ["String"] }`.
The unit type `()` is the string `"()"`. Tuple types appear as
`{ "tuple": ["String", "int64"] }`.

#### `machines[]`

Each machine entry contains:

| Field | Type | Description |
|---|---|---|
| `name` | string | Machine name |
| `generic_params` | array | Generic type parameter names and bounds |
| `sends` | array of strings | Channel names this machine sends to |
| `receives` | array of strings | Channel names this machine reads from |
| `supervises` | array | Child machines and their supervision strategies |
| `states` | array | State declarations with field lists |
| `transitions` | array | Transition declarations with from/target/timeout |
| `effects` | array | Effect and action declarations |
| `handlers` | array | `on` handler bodies (full AST) |

#### `machines[].states[]`

```json
{
  "name": "Processing",
  "fields": [
    { "name": "order_id", "type": "String" }
  ]
}
```

State fields are the data carried by that state. When a runtime checkpoints a
machine it must snapshot the current state name together with the fields of the
active state.

#### `machines[].transitions[]`

```json
{
  "name": "complete",
  "from": "Processing",
  "targets": ["Done", "Faulted"],
  "timeout": null
}
```

`targets` lists every state the transition can reach. A transition with a
`timeout` carries `{ "value": 30, "unit": "s" }` (units: `"ms"`, `"s"`,
`"m"`, `"h"`).

#### `machines[].effects[]`

This is the most important array for workflow runtimes. Each entry describes one
declared effect or action:

```json
{
  "name": "validate_order",
  "params": [
    { "name": "order_id", "type": "String" }
  ],
  "return_type": "bool",
  "is_async": false,
  "kind": "effect"
}
```

```json
{
  "name": "charge_card",
  "params": [
    { "name": "order_id",     "type": "String" },
    { "name": "amount_cents", "type": "int64"  }
  ],
  "return_type": "String",
  "is_async": false,
  "kind": "action"
}
```

The `kind` field is the primary signal for replay semantics. Its values are:

| `kind` | Meaning |
|---|---|
| `"effect"` | Idempotent / replay-safe. A replay-aware runtime may re-execute on replay without checkpointing the result. |
| `"action"` | Non-idempotent / externally visible. **Must be checkpointed.** On replay, restore the cached result instead of re-executing. |

The `is_async` flag indicates that the generated Go interface method takes a
`context.Context` as its first parameter and returns an `error` (or
`(T, error)` for non-unit return types).

---

## `effect` vs `action` Replay Semantics

### The fundamental distinction

A Gust machine's handler can call out to the world through two kinds of
declarations:

- **`effect`** — assumed idempotent. Querying a read-only API, computing a hash,
  reading from a cache. A replay-aware runtime can re-execute these freely
  because running them again produces the same result.

- **`action`** — non-idempotent / externally visible. Charging a card, sending
  an email, writing to a database. Running these a second time changes the world.
  A workflow runtime must checkpoint the result after the first successful
  execution and replay the cached value instead of calling the implementation
  again.

### What the runtime must do

```
On first execution:
  1. Execute the action implementation.
  2. Checkpoint (store) the return value alongside the workflow execution ID
     and the step within the handler.
  3. Transition the machine to the next state.

On replay (e.g. after a crash and restart):
  1. Walk the handler body from the top.
  2. When reaching an `action` perform:
     a. Look up the checkpoint for this (workflow_id, step) pair.
     b. If found: return the cached value — do NOT call the implementation.
     c. If not found: treat as first execution (step 1 above).
  3. `effect` performs: always re-execute — no checkpoint lookup needed.
```

### Validator-enforced handler-safety rules

The Gust validator emits **warnings** (not errors) when a handler violates the
two action-safety rules. Runtimes should treat these warnings as blocking during
contract import — a handler that violates them cannot be safely checkpointed.

**Rule 1 — at most one `action` per code path.**

More than one action in a single linear sequence means the runtime would need to
checkpoint multiple external calls in sequence. This is not forbidden, but the
validator warns because it is a frequent source of checkpointing bugs in
practice.

The following `.gu` fragment triggers Rule 1 — two actions in the same linear
sequence:

```gust
machine PaymentProcessor {
    state Processing(order_id: String)
    state Done(receipt: String, notification: String)

    transition process: Processing -> Done

    action charge_card(order_id: String, amount_cents: i64) -> String
    action send_email(email: String, msg: String) -> String

    on process(ctx: ProcessCtx, email: String) {
        let receipt = perform charge_card(ctx.order_id, 1000);
        let notif = perform send_email(email, receipt);
        goto Done(receipt, notif);
    }
}
```

This version passes the validator — one action per path, effect steps before it:

```gust
machine PaymentProcessor {
    state Processing(order_id: String)
    state Done(receipt: String)

    transition process: Processing -> Done

    effect validate_order(order_id: String) -> bool
    action charge_card(order_id: String, amount_cents: i64) -> String

    on process(ctx: ProcessCtx) {
        let valid = perform validate_order(ctx.order_id);
        if valid {
            let receipt = perform charge_card(ctx.order_id, 1000);
            goto Done(receipt);
        } else {
            goto Done("skipped");
        }
    }
}
```

**Rule 2 — an `action` must be the last side-effectful step before a
transition.**

If an action is followed by another externally visible step (another effect,
another action, a `send`, a `spawn`) then the checkpoint boundary is ambiguous:
the runtime cannot know whether the post-action steps already ran when it
resumes.

The following triggers Rule 2 — an effect follows an action:

```gust
machine PaymentProcessor {
    state Processing(order_id: String)
    state Done(receipt: String, config: String)

    transition process: Processing -> Done

    effect lookup_config(order_id: String) -> String
    action charge_card(order_id: String, amount_cents: i64) -> String

    on process(ctx: ProcessCtx) {
        let receipt = perform charge_card(ctx.order_id, 1000);
        let config = perform lookup_config(ctx.order_id);
        goto Done(receipt, config);
    }
}
```

This version is correct — all effects precede the action:

```gust
machine PaymentProcessor {
    state Processing(order_id: String)
    state Done(receipt: String, config: String)

    transition process: Processing -> Done

    effect lookup_config(order_id: String) -> String
    action charge_card(order_id: String, amount_cents: i64) -> String

    on process(ctx: ProcessCtx) {
        let config = perform lookup_config(ctx.order_id);
        let receipt = perform charge_card(ctx.order_id, 1000);
        goto Done(receipt, config);
    }
}
```

Note that branches (if/else, match arms) are analyzed independently. An action
in one branch and a different action in a sibling branch do not trigger Rule 1
— they are on separate code paths.

---

## `EngineFailure` as a Stdlib Contract

`EngineFailure` is a Gust standard library enum that gives runtimes a stable,
typed failure surface. Without it, runtimes would need to parse arbitrary strings
to decide whether to retry, escalate, or cancel a workflow.

### Importing the type

```gust
use std::EngineFailure;

machine FailureDemo {
    state Running
    state Faulted(failure: EngineFailure)

    transition fail: Running -> Faulted

    effect make_failure(reason: String) -> EngineFailure

    on fail(reason: String) {
        let f = perform make_failure(reason);
        goto Faulted(f);
    }
}
```

After this import the `EngineFailure` type is available anywhere in the file.

### The five variants

All variant payloads are **positional** (Gust enums currently do not support
named fields in variant positions — see
[Known Limitations](../appendix/known_limitations.md)).

| Variant | Payload (in order) | Meaning |
|---|---|---|
| `UserError` | `reason: String` | The workflow failed due to caller-supplied invalid input. Do not retry. |
| `SystemError` | `reason: String`, `attempt: i64` | A transient internal failure. `attempt` is 1-based and intended for retry policy decisions. |
| `IntegrationError` | `service: String`, `status_code: i64`, `body: String` | A downstream service returned an unexpected response. `service` is the identifier of the failing service; `status_code` is HTTP-style; `body` is the raw response. |
| `Timeout` | `wall_clock_ms: i64` | The operation exceeded its deadline. `wall_clock_ms` is elapsed time at the point the timeout fired. |
| `Cancelled` | `requested_by: String` | The workflow was cancelled. `requested_by` is an opaque identifier for the cancellation source (user ID, system, etc.). |

### Using `EngineFailure` in a machine

The following machine carries `EngineFailure` in its `Faulted` state and
produces a failure value through an `action` (the failure recording step is
externally visible — it writes to an observability store):

```gust
use std::EngineFailure;

machine OrderProcessor {
    state Processing(order_id: String)
    state Done(receipt: String)
    state Faulted(failure: EngineFailure)

    transition complete: Processing -> Done | Faulted

    effect lookup_config(order_id: String) -> String
    action charge_card(order_id: String, amount_cents: i64) -> String
    action record_failure(reason: String, attempt: i64) -> EngineFailure

    on complete(ctx: CompleteCtx) {
        let config = perform lookup_config(ctx.order_id);
        if config == "" {
            let failure = perform record_failure("config missing", 1);
            goto Faulted(failure);
        } else {
            let receipt = perform charge_card(ctx.order_id, 1000);
            goto Done(receipt);
        }
    }
}
```

The runtime implementation of `record_failure` writes to a structured log and
returns the `EngineFailure` value the machine carries into the `Faulted` state.

### Wrapping `EngineFailure` in a domain enum

Runtimes or application teams that need additional domain-specific variants can
wrap `EngineFailure` rather than replacing it:

```gust
use std::EngineFailure;

enum PaymentFailure {
    Engine(EngineFailure),
    CardDeclined(String),
    InsufficientFunds(i64),
}

machine PaymentProcessor {
    state Processing(order_id: String)
    state Done(receipt: String)
    state Faulted(failure: PaymentFailure)

    transition complete: Processing -> Done | Faulted

    action charge_card(order_id: String, amount_cents: i64) -> String
    action record_failure(reason: String) -> PaymentFailure

    on complete(ctx: CompleteCtx) {
        let failure = perform record_failure("declined");
        goto Faulted(failure);
    }
}
```

The `Engine(EngineFailure)` variant preserves the standard failure contract for
the runtime's retry and observability infrastructure, while the domain variants
(`CardDeclined`, `InsufficientFunds`) carry business-specific context.

---

## Go Codegen Pipeline

### Compiling a machine to Go

```bash
gust build path/to/machine.gu --target go --package mypkg
```

This writes `machine.g.go` in the current directory (or the directory specified
with `--output`). The `.g.go` extension signals that the file is generated and
must not be manually edited.

You can also invoke the `gust_build` MCP tool from the runtime's own build
tooling:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "gust_build",
    "arguments": {
      "file": "/path/to/machine.gu",
      "target": "go",
      "package": "mypkg"
    }
  }
}
```

### What the generated file contains

For a machine named `OrderProcessor` in package `mypkg`, the generated file
produces the following declarations:

**State constants via iota**

```go
type OrderProcessorState int

const (
    OrderProcessorStateIdle = iota
    OrderProcessorStateProcessing
    OrderProcessorStateDone
    OrderProcessorStateFaulted
)
```

**State data structs** — one per state that carries fields

```go
type OrderProcessorProcessingData struct {
    OrderId string `json:"order_id"`
}

type OrderProcessorDoneData struct {
    Receipt string `json:"receipt"`
}
```

**Effects interface** — one method per declared `effect` or `action`. Each
method gets a marker comment so downstream tooling can identify replay semantics
without re-parsing the `.gu` source:

```go
type OrderProcessorEffects interface {
    // gust:effect -- replay-safe / idempotent
    ValidateOrder(order_id string) bool
    // gust:action -- not replay-safe / externally visible
    ChargeCard(order_id string, amount_cents int64) (string, error)
}
```

The `gust:<kind>` marker appears on the line immediately before each effect or
action method. Runtimes that generate adapters from the interface can detect
these comments to automatically choose replay or checkpoint behavior.

**Machine struct**

```go
type OrderProcessor struct {
    State          OrderProcessorState           `json:"state"`
    ProcessingData *OrderProcessorProcessingData `json:"processing_data,omitempty"`
    DoneData       *OrderProcessorDoneData       `json:"done_data,omitempty"`
}
```

The struct is JSON-serializable out of the box. The runtime can marshal it with
`m.ToJSON()` and unmarshal a checkpoint with `OrderProcessorFromJSON(data)`.

**Transition methods** with runtime state validation

```go
func (m *OrderProcessor) Complete(effects OrderProcessorEffects) error {
    if m.State != OrderProcessorStateProcessing {
        return &OrderProcessorError{Transition: "complete", From: m.State.String()}
    }
    // ... handler body ...
    return nil
}
```

Each method returns `error`. An invalid transition (calling `Complete` when the
machine is in `Idle`) returns an `*OrderProcessorError` with the transition name
and the actual current state. Any error propagated from an effect or action
implementation is returned directly.

### Embedding generated code in a workflow runtime

The generated file is a self-contained Go package fragment. To integrate it into
a larger runtime:

1. Place `machine.g.go` in the same Go package as your workflow executor.
2. Implement the `OrderProcessorEffects` interface. The runtime provides one
   implementation per execution context: the implementation of each action method
   first checks the checkpoint store for a cached result and returns it if
   present; otherwise it calls the real implementation and writes the result to
   the checkpoint store before returning.
3. Hydrate a machine instance from a checkpoint with `OrderProcessorFromJSON`, or
   create a fresh one with `NewOrderProcessor`.
4. Call transition methods in response to incoming events. After each successful
   transition, serialize the machine state with `m.ToJSON()` and persist it as
   the new checkpoint.
5. On replay (after a crash), restore the machine from the last checkpoint, then
   replay any events that arrived after the checkpoint. Actions will find their
   results in the checkpoint store and skip re-execution.

This pattern keeps the generated Gust code ignorant of the runtime's persistence
layer — the machine only knows about effects and state; all checkpointing is in
the effects interface implementation.

---

## References

- [Language Reference](../reference/README.md) — complete syntax, types, and
  semantics
- [Effects and Handlers](../reference/effects_handlers.md) — full reference for
  `effect`, `action`, `perform`, and handler bodies
- [Stdlib API](../appendix/stdlib_api.md) — all standard library machines and
  types including `EngineFailure`
- [Changelog](../appendix/changelog.md) — version history
- [`gust-mcp/src/lib.rs`](../../gust-mcp/src/lib.rs) — source of truth for
  `gust_parse` JSON field names and the `serialize_program` function
- [`gust-stdlib/engine_failure.gu`](../../gust-stdlib/engine_failure.gu) —
  canonical `EngineFailure` definition with variant payload documentation
