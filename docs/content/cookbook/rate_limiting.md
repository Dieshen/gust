---
title: "Rate Limiting"
description: "Token-bucket admission control as a two-state machine, where exhaustion is a state the caller can see rather than an error it has to interpret."
type: guide
---

# Rate Limiting

You are allowed 100 requests a minute to a partner API. Exceeding it does not fail gracefully — it gets your key throttled, or banned.

The token bucket is the standard answer: hold a count, spend one per operation, refill on a schedule. Modelling it as a machine buys you one thing a counter does not give you — **exhausted is a state**, not an error code you have to translate at every call site. Code that asks "may I proceed?" gets an answer it can pattern-match.

## The machine

Two states, and the whole policy fits in four lines of handler.

```gust
machine ApiRateLimiter {
    state Available(tokens: i64, max_tokens: i64)
    state Exhausted(retry_after_ms: i64, max_tokens: i64)

    transition acquire: Available -> Available | Exhausted
    transition refill: Exhausted -> Available

    effect now_ms() -> i64

    on acquire(ctx) {
        if ctx.tokens > 0 {
            goto Available(ctx.tokens - 1, ctx.max_tokens);
        } else {
            goto Exhausted(perform now_ms(), ctx.max_tokens);
        }
    }

    on refill(ctx) {
        goto Available(ctx.max_tokens, ctx.max_tokens);
    }
}
```

Save as `rate_limiter.gu` and build. `gust check` passes with no warnings — this is the smallest machine in the cookbook.

Note that `acquire` spends a token *and* reports the outcome in one transition. You do not ask permission and then take; the state after `acquire` tells you what happened.

## What the host implements

One effect: the clock.

```rust "src/limiter_effects.rs"
impl ApiRateLimiterEffects for SystemClock {
    fn now_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before epoch")
            .as_millis() as i64
    }
}
```

## Driving it

Refill is a poll, not a timer — Gust runs no background clock, so your host decides when a window has elapsed.

```rust "src/gateway.rs"
struct Gateway {
    limiter: ApiRateLimiter,   // ApiRateLimiter::new(100, 100)
    clock: SystemClock,
}

impl Gateway {
    async fn call(&mut self) -> Result<Response, Rejected> {
        self.limiter.acquire(&self.clock)?;
        match self.limiter.state() {
            ApiRateLimiterState::Available { tokens, .. } => {
                tracing::trace!("admitted, {tokens} left");
                Ok(call_partner_api().await)
            }
            ApiRateLimiterState::Exhausted { retry_after_ms, .. } => {
                Err(Rejected::RateLimited { since_ms: *retry_after_ms })
            }
        }
    }

    /// Called from a tokio::time::interval every 60s.
    fn refill_window(&mut self) -> Result<(), ApiRateLimiterError> {
        if matches!(self.limiter.state(), ApiRateLimiterState::Exhausted { .. }) {
            self.limiter.refill()?;
        }
        Ok(())
    }
}
```

`refill` takes no `effects` argument because its handler performs none. Note that `call` reads `state()` but never fires a transition inside the `match` — `state()` borrows the machine and a transition needs `&mut self`, so use `matches!` when you need to decide *and* act.

## Reading `retry_after_ms` honestly

The field name says "retry after", but the value stored is `perform now_ms()` — the wall-clock instant at which the bucket ran dry, not a duration the caller should wait. The stdlib machine has the same mismatch.

Pick one and be consistent:

- **Keep the timestamp** and have the caller compute `window_ms - (now - retry_after_ms)`. Rename the field `exhausted_at_ms` so it stops lying.
- **Store the wait** by declaring `effect ms_until_refill() -> i64` and performing that instead. The caller can then hand the value straight to a `Retry-After` header.

The second is friendlier to callers; the first survives serialisation across a process restart without becoming stale.

## The stdlib version

`gust-stdlib/rate_limiter.gu` is this machine as `machine RateLimiter<K>`, generic over a key type, using bare source-state field references.

The `<K>` never appears in a state field or an effect signature — it was presumably intended for per-key buckets that the machine does not actually implement. That makes the Rust output uncompilable: `RateLimiterState<K>` fails with `E0392: type parameter 'K' is never used`, and every state assignment then fails inference. The Go output builds and vets clean. Dropping `<K>`, as the recipe does, loses nothing.

## Tuning

- **One machine per key.** A single machine is one bucket. Per-tenant limiting means a `HashMap<TenantId, ApiRateLimiter>` in the host, not a generic parameter on the machine.
- **This is a fixed-window bucket, not a sliding one.** `refill` restores the full allowance at once, so a caller can spend 100 tokens at 59s and 100 more at 61s. If that burst matters, refill in smaller increments more often, which needs an `Available -> Available` top-up transition.
- **Rate limiting goes in front of everything else.** The point is to not make the call. Check admission before the [circuit breaker](./circuit_breaker.md), before [retry](./retry.md), before the [request](./request_response.md).
