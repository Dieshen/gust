# Code Generation

Gust compiles the same machine model into multiple target-specific outputs.

## Rust target

`gust build --target rust ./order.gu`

Rust generation emphasizes strong typing and async ergonomics:
- state enum + machine struct
- transition methods with runtime from-state validation
- `tokio` timeout wiring when transition timeout is declared
- typed channel and supervisor parameters when `send`/`spawn` are used

## Go target

`gust build --target go --package orders ./order.gu`

Go generation emits:
- state constants + data structs
- channel wrappers for `broadcast` and `mpsc`
- `context.WithTimeout` scaffold for transition timeouts
- supervisor interface wiring for `spawn`

## Shared constraints

Both Rust and Go codegen consume the same AST. That means parser/validator behavior is consistent across targets, while runtime idioms differ by language.

## Snippet

```gust
channel Events: String (capacity: 64, mode: broadcast)

machine Producer(sends Events) {
    state Idle
    state Done

    transition run: Idle -> Done timeout 2s

    on run() {
        send Events("start");
        goto Done();
    }
}
```
