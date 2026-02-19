# Event Processor Example

A working example of a Gust state machine that models a multi-stage event
processing pipeline. The machine enforces that events flow through validation
before processing, routes low-priority events to a Failed state, and supports
recovery via retry.

## State Machine Design

```
Idle
 |
 | receive(Event)
 v
Receiving
 |
 | validate [priority > 0]      [priority <= 0]
 +-----------------------> Validating   -------> Failed
                               |                    |
                               | process            | retry
                               v                    v
                           Completed              Idle
                               |
                               | reset
                               v
                             Idle
```

**States:**
- `Idle` — waiting for an event
- `Receiving(event)` — event accepted, awaiting validation
- `Validating(event)` — validation passed, ready for processing
- `Completed(result)` — processing finished; holds the ProcessedResult
- `Failed(reason)` — validation rejected the event

**Effects (implemented by the caller):**
- `validate_event(event) -> String` — returns a validation token; the handler
  checks `event.priority > 0` to decide whether to proceed or fail
- `process_event(event) -> ProcessedResult` — transforms the event into a result

## Running

```bash
cargo run
```

Example output:

```
=== Event Processor Example ===

-- Happy path: valid high-priority event --
Initial state: Idle
After receive: Receiving { event: ... }
After validate: Validating { event: ... }
After process: Completed { result: ProcessedResult { event_id: "sensor-1-16", ... } }
Result: event_id=sensor-1-16, output=processed(sensor-1) -> TEMPERATURE:98.6
After reset: Idle

-- Failure path: zero-priority event rejected at validate --
After validate (bad priority): Failed { reason: "event priority must be positive" }
After retry: Idle

-- Invalid transition: attempt validate from Idle --
Got expected error: invalid transition 'validate' from state 'Idle'

All examples completed successfully.
```

## Testing

```bash
cargo test
```

Tests cover:
- Happy path: full pipeline from Idle to Completed to Idle
- Failure path: zero-priority event routed to Failed
- Retry recovery: Failed -> Idle -> successful processing
- Invalid transitions: calling transitions from the wrong state returns `InvalidTransition`
- Negative priority also routes to Failed

## Structure

```
src/
  processor.gu       # Gust machine definition
  processor.g.rs     # Generated Rust (do not edit)
  main.rs            # Effects implementation and demo
tests/
  test.rs            # Integration tests
build.rs             # Calls gust_build::compile_gust_files()
```
