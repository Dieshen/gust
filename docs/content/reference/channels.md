---
title: "Channels"
description: "Declaring typed channels, sending messages from a handler, and what the sends and receives annotations generate in Rust and Go."
type: reference
---

# Channels

A channel is a typed, program-scope message queue. Machines declare their
relationship to one in the machine header and push values with `send`.

Channels are the in-process transport only. Cross-process and network transport
are deliberately out of scope; see
[Known Limitations](../appendix/known_limitations.md).

## Declaring a channel

```
channel Orders: Order
channel Events: Order (capacity: 64, mode: broadcast)
```

Channel declarations live at the top level, alongside `type` and `machine`, and
take **no trailing semicolon** — `channel Orders: Order;` is a parse error.

| Setting | Values | Default |
|---|---|---|
| `mode` | `broadcast`, `mpsc` | `broadcast` |
| `capacity` | a positive integer | `1024` (a declared `0` is clamped to `1`) |

The message type is any [type expression](types.md#type-expressions).

## Sending

`send` takes a channel name and **exactly one** argument:

```gust
channel Orders: String

machine Producer {
    state Idle
    state Sent

    transition emit: Idle -> Sent

    on emit(payload: String) {
        send Orders(payload);
        goto Sent;
    }
}
```

The channel named by `send` must be declared at program scope; an unknown name
is a hard error with a did-you-mean suggestion. It does **not** have to appear in
the machine's `sends` annotation — the two are checked independently.

## Machine annotations

```
machine Producer(sends Orders) { ... }
machine Consumer(receives Orders) { ... }
machine Coordinator(sends Events, receives Commands) { ... }
```

Both annotations must name a channel declared at program scope. This is a hard
error rather than a warning, because both backends resolve the name by lookup and
a miss silently omits the generated helper:

```
error: undeclared channel 'Order' in 'sends' annotation on machine 'Producer'
   = note: a 'sends' annotation must name a channel declared at program scope;
           declared channels: Orders
   = help: did you mean 'Orders'?
```

`receives` is metadata. Neither backend generates anything from it — it records
intent for tooling and for readers.

## What gets generated

### The channel type

Each declaration produces a `<Name>Channel` type in both backends.

| Mode | Rust | Go |
|---|---|---|
| `broadcast` | wraps `tokio::sync::broadcast::Sender`; `new`, `sender`, `send`, `subscribe` | `Subscribe() <-chan T`, `Publish(msg T)`, guarded by a `sync.RWMutex` |
| `mpsc` | wraps `tokio::sync::mpsc::Sender` plus a mutex-guarded receiver; `new`, `sender`, `try_send`, `receive` | `Send(msg T) bool`, `Receive() <-chan T` |

Neither backend's send path blocks or reports back-pressure: Rust discards the
send result, and Go's broadcast `Publish` drops the message when a subscriber's
buffer is full.

### The transition method parameter

A handler that contains a `send` makes the generated transition method take the
channel as a parameter. The caller supplies it.

| | Rust | Go |
|---|---|---|
| Parameter | `orders_tx: &tokio::sync::broadcast::Sender<T>` | `ordersCh *OrdersChannel` |
| Naming | channel name in `snake_case`, suffixed `_tx` | channel name in `snake_case`, suffixed `Ch` |

Several `send`s to different channels add several parameters, ordered by channel
name.

### The `sends` helper

A `sends` annotation additionally generates a helper so a machine can push to the
channel outside a transition — `SendOrders(msg, ch)` in Go.

In Rust the helper is an inherent method on the machine, so both backends
describe the same API.

## Cross-backend notes

Declaring a channel also changes the generated preludes. Rust references
`tokio::sync::*` through fully qualified paths, so no `use tokio;` is emitted and
the consuming crate must depend on `tokio` itself. A `broadcast` channel makes
the Go backend import `sync`.

## Next

- [Supervision](supervision.md) — the other machine-header annotation
- [Syntax](syntax.md#file-structure) — where channel declarations may appear
- [Errors](errors.md) — these diagnostics in context
