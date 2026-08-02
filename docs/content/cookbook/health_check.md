---
title: "Health Check"
description: "Track whether a dependency is up with a degraded middle state, so one failed probe is not treated the same as a sustained outage."
type: guide
---

# Health Check

A single failed probe means very little. A network hiccup, a pod restarting, a slow GC pause — treat any of them as "the service is down" and you will page somebody at three in the morning for nothing.

What you want is a middle state: healthy, *degraded*, and unhealthy. Degraded says "a probe failed and I am watching". Only a second failure escalates. Writing it as a machine puts that escalation rule in one visible place instead of spreading it across a counter, an `if`, and an alerting threshold.

## The machine

`ServiceHealth` keeps the last known good status, so a degraded service still reports something useful.

```gust
machine ServiceHealth {
    state Healthy(status: String)
    state Degraded(status: String, failures: i64)
    state Unhealthy(reason: String)

    transition probe: Healthy -> Healthy | Degraded
    transition recheck: Degraded -> Healthy | Unhealthy

    async effect run_probe() -> Result<String, String>

    async on probe(ctx: ProbeCtx) {
        let result = perform run_probe();
        match result {
            Ok(next_status) => {
                goto Healthy(next_status);
            }
            Err(_) => {
                goto Degraded(ctx.status, 1);
            }
        }
    }

    async on recheck(ctx: RecheckCtx) {
        let result = perform run_probe();
        match result {
            Ok(next_status) => {
                goto Healthy(next_status);
            }
            Err(reason) => {
                goto Unhealthy(reason);
            }
        }
    }
}
```

Save as `service_health.gu` and build. `gust check` warns that both handlers have code paths that don't end with a `goto`; every `match` arm does, and the terminator analysis does not look inside them.

The `Err(_)` in `probe` is deliberate. `Degraded` keeps the *last good* status rather than the error, so binding a name you never read would leave an unused variable — a `clippy -D warnings` error in Rust and a hard error in Go. Bind `_` when you genuinely do not need the value.

## What the host implements

One effect. Whatever "is it up" means for this dependency lives here.

```rust "src/health_effects.rs"
impl ServiceHealthEffects for PostgresProbe {
    async fn run_probe(&self) -> Result<String, String> {
        let row: (i64,) = sqlx::query_as("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!("ok (sentinel {})", row.0))
    }
}
```

Return something descriptive rather than a bare `"ok"` — the string ends up in the `Healthy` state, which is what your status endpoint will serialise.

## Driving it

Gust runs no background clock, so the probe interval is yours.

```rust "src/health.rs"
let mut health = ServiceHealth::new("startup".to_string());
let mut ticker = tokio::time::interval(Duration::from_secs(10));

loop {
    ticker.tick().await;

    if matches!(health.state(), ServiceHealthState::Degraded { .. }) {
        health.recheck(&effects).await?;
    } else if matches!(health.state(), ServiceHealthState::Healthy { .. }) {
        health.probe(&effects).await?;
    }

    if let ServiceHealthState::Unhealthy { reason } = health.state() {
        alert(reason);
        break;
    }
}
```

Drive the machine with `matches!` rather than from inside a `match` on `state()`. `state()` borrows the machine and a transition needs `&mut self`, so calling one inside a `match` arm is a borrow-checker error.

There is no way out of `Unhealthy` — no transition names it as a source. That is the recipe being explicit rather than incomplete: recovering from a declared outage usually means recreating the connection pool, not flipping a state field. If you want in-place recovery, add `transition revive: Unhealthy -> Healthy | Unhealthy` with the same probe body.

## The stdlib version

`gust-stdlib/health_check.gu` is `machine HealthCheck<T>`, generic over the status payload, using bare field references. Two differences beyond the generics:

- Its `probe` transition declares `Healthy -> Healthy | Degraded | Unhealthy`, but no path in the handler reaches `Unhealthy`. The recipe drops the unreachable target so the generated diagram is honest.
- Its `Err` arm binds `err` and never uses it, which is an unused-variable error under `clippy -D warnings`.

The generic version does not produce compiling Rust: `let status = status.clone();` on an unbounded `T` yields `&T` where `T` is expected. The Go output builds and vets clean.

## Tuning

- **`failures: 1` is a placeholder.** The recipe transitions `Healthy -> Degraded` with a hard-coded count because `probe` cannot re-enter `Degraded`. If you want "three strikes", add `transition probe_again: Degraded -> Degraded | Unhealthy` and increment `ctx.failures` on the way through.
- **Probe the thing you actually depend on.** `SELECT 1` proves the connection pool works, not that the query your service runs will succeed. Probe the narrowest operation that would page you.
- **Do not probe on the request path.** The interval loop above keeps probe cost fixed regardless of traffic, which is the point.
- **Feed the breaker rather than replacing it.** A [circuit breaker](./circuit_breaker.md) reacts to real traffic failing; a health check reacts to a synthetic probe. They answer different questions and are worth running together.
