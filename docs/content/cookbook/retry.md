# Retry

Retry is useful for transient failures. The stdlib Retry machine models bounded attempts plus backoff.

## Model

`gust-stdlib/retry.gu` includes:
- `Attempting` state for operation execution
- `Waiting` state for delay between attempts
- `compute_backoff(...)` effect for strategy control
- `sleep_ms(...)` effect for async waiting

## Usage sketch

```gust
machine Client {
    state Idle
    state Running(retry: Retry<String>)
    state Done
    state Failed(reason: String)

    transition begin: Idle -> Running

    on begin() {
        goto Running(retry);
    }
}
```

## Backoff guidance

- cap maximum delay
- include jitter for thundering-herd mitigation
- keep retry budget finite to protect downstream systems
