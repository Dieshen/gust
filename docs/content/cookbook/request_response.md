---
title: "Request / Response"
description: "Model a single outbound call and its reply as a state machine, so pending, failed, and timed-out are three distinguishable states."
type: guide
---

# Request / Response

You call another service and wait for one reply. Three things can happen: the reply arrives, the call fails, or nothing comes back before your deadline. Written as ordinary code those collapse into a single `Result`, and "still waiting" is not represented at all.

Modelling the call as a machine keeps all four situations distinguishable, and makes "in flight" something you can serialise, log, and resume.

## The machine

`ApiCall` starts in `Pending`, carrying the request and the deadline it was issued with. Exactly one of two transitions moves it out.

```gust
type ApiRequest { id: String, url: String }
type ApiResponse { id: String, body: String }

machine ApiCall {
    state Pending(request: ApiRequest, timeout_ms: i64)
    state Completed(response: ApiResponse)
    state Failed(error: String)
    state TimedOut(elapsed_ms: i64)

    transition receive: Pending -> Completed | Failed
    transition give_up: Pending -> TimedOut

    async effect wait_for_response(request: ApiRequest, timeout_ms: i64) -> Result<ApiResponse, String>
    effect elapsed_ms(started_at_ms: i64) -> i64

    async on receive(ctx) {
        let result = perform wait_for_response(ctx.request, ctx.timeout_ms);
        match result {
            Ok(response) => {
                goto Completed(response);
            }
            Err(err) => {
                goto Failed(err);
            }
        }
    }

    on give_up(ctx) {
        goto TimedOut(perform elapsed_ms(ctx.timeout_ms));
    }
}
```

Save that as `api_call.gu`, then run `gust check api_call.gu && gust build api_call.gu`.

`TimedOut` is reached by its own transition, not by the reply path. Gust's `timeout` clause on a transition is a watchdog on handler execution — it returns `Err` and leaves the machine where it was, so it cannot deliver you into a `TimedOut` state. A deadline you want to observe has to be a transition you fire.

## What the host implements

The generated `ApiCallEffects` trait is where the HTTP lives. Arguments arrive borrowed, and the async effect becomes a returned future.

```rust "src/api_call_effects.rs"
impl ApiCallEffects for HttpEffects {
    async fn wait_for_response(
        &self,
        request: &ApiRequest,
        timeout_ms: i64,
    ) -> Result<ApiResponse, String> {
        let call = self.client.get(&request.url).send();
        match tokio::time::timeout(Duration::from_millis(timeout_ms as u64), call).await {
            Ok(Ok(resp)) => Ok(ApiResponse {
                id: request.id.clone(),
                body: resp.text().await.map_err(|e| e.to_string())?,
            }),
            Ok(Err(e)) => Err(e.to_string()),
            Err(_) => Err("deadline exceeded".to_string()),
        }
    }

    fn elapsed_ms(&self, started_at_ms: i64) -> i64 {
        self.now_ms() - started_at_ms
    }
}
```

The machine does not know what a deadline is. It knows that `wait_for_response` returns either a response or a string, and that `give_up` exists. The meaning of both is yours to supply.

## Driving it

`ApiCall::new` takes the fields of the first declared state, so it seeds `Pending` directly.

```rust "src/main.rs"
let mut call = ApiCall::new(request, 2_000);
call.receive(&effects).await?;

match call.state() {
    ApiCallState::Completed { response } => deliver(response),
    ApiCallState::Failed { error } => tracing::warn!("call failed: {error}"),
    ApiCallState::TimedOut { elapsed_ms } => tracing::warn!("gave up after {elapsed_ms}ms"),
    ApiCallState::Pending { .. } => unreachable!("receive always leaves Pending"),
}
```

Calling `receive` again from `Completed` returns `Err(ApiCallError::InvalidTransition { .. })` rather than panicking, which is what makes the machine safe to hand to a retry loop.

## The stdlib version

`gust-stdlib/request_response.gu` is the same shape, generic over the request and response types. Three differences from the recipe above:

- It declares `machine RequestResponse<T, R>`, with `Pending(request: T, timeout_ms: i64)` and `Completed(response: R)`.
- It has a `send: Pending -> Pending` self-transition that re-enters `Pending` unchanged. It lets a caller record an attempt without changing state; the recipe drops it because it does nothing observable.
- Its `timeout` handler stores `perform current_time_ms()` into a field called `elapsed_ms`, so the field actually holds an absolute timestamp. The recipe uses an `elapsed_ms` effect so the name and the value agree.

The generic version does not currently produce compiling Rust, and the `send` self-transition is exactly where it breaks. Codegen emits `let request = request.clone();` and stores the result back into `Pending`, but the generated `impl` only bounds `T: Debug` — so `.clone()` on a `&T` resolves to `Clone for &T` and yields `&T` where `T` is expected. Dropping `send`, as the recipe does, removes the only place a generic-typed field is carried forward. The Go output builds and vets clean either way.

## Tuning

- **Put the deadline in the state, not in the caller.** `Pending` carries `timeout_ms`, so a machine rehydrated from JSON still knows its own contract.
- **Give `Failed` a reason string rather than a boolean.** The state enum and the generated diagram both become honest about why.
- **Do not add a `retry` transition here.** A request that should be retried is a [retry](./retry.md) machine wrapping this one, so the attempt count lives somewhere it can be bounded.
