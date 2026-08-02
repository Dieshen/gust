---
title: "Supervision"
description: "Feed uploads over a channel and put a supervising Intake machine in front of the worker."
type: tutorial
---

# Supervision

One `Upload` handles one photo. A service handles a queue of them, and needs somewhere to put the question "what happens when a worker dies?".

Gust has two constructs for this. A **channel** carries values between machines. A **supervisor** is a machine that declares which machines it owns and how they should be restarted. On this page you add one of each.

## Declare the channel and the supervisor

Add a channel at the top level, mark `Upload` as its receiver, and add a second machine that owns the workers.

```gust
type Photo {
    id: String,
    bytes: i64,
}

channel Uploads: Photo (capacity: 32, mode: mpsc)

machine Upload(receives Uploads) {
    state Waiting
    state Received(photo: Photo)
    state Scanned(photo: Photo)
    state Published(photo: Photo, url: String)
    state Rejected(photo: Photo, reason: String)

    transition receive: Waiting -> Received
    transition scan: Received -> Scanned | Rejected
    transition publish: Scanned -> Published
    transition discard: Rejected -> Waiting

    effect scan_photo(photo: Photo) -> bool
    async effect store_photo(photo: Photo) -> String

    action notify_uploader(photo_id: String, url: String) -> ()

    on receive(photo: Photo) {
        goto Received(photo);
    }

    on scan(ctx: ScanCtx) {
        if ctx.photo.bytes > 8000000 {
            goto Rejected(ctx.photo, "larger than the 8 MB limit");
        } else if perform scan_photo(ctx.photo) {
            goto Scanned(ctx.photo);
        } else {
            goto Rejected(ctx.photo, "failed the malware scan");
        }
    }

    async on publish(ctx: PublishCtx) {
        let url = perform store_photo(ctx.photo);
        perform notify_uploader(ctx.photo.id, url);
        goto Published(ctx.photo, url);
    }

    on discard() {
        goto Waiting;
    }
}

machine Intake(supervises Upload(one_for_one)) {
    state Booting
    state Serving(workers: i64)
    state Draining(workers: i64)

    transition start: Booting -> Serving
    transition drain: Serving -> Draining

    on start(worker_count: i64) {
        spawn Upload();
        goto Serving(worker_count);
    }

    on drain(ctx: DrainCtx) {
        goto Draining(ctx.workers);
    }
}
```

That is the complete `src/upload.gu` you finish the tutorial with. Reading the new parts:

- **`channel Uploads: Photo (capacity: 32, mode: mpsc)`** — a bounded queue of `Photo`. `mpsc` means each message goes to exactly one consumer, which is what you want for distributing work; `broadcast` means every consumer sees every message. Channel declarations take **no** trailing semicolon.
- **`machine Upload(receives Uploads)`** — an annotation recording that this machine consumes the channel.
- **`machine Intake(supervises Upload(one_for_one))`** — `Intake` owns `Upload` children and restarts them one at a time.
- **`spawn Upload();`** — starts a child. The spawn target must be a machine declared in the same file, which is why this listing includes both.

The restart strategies are the Erlang ones. Given children `[A, B, C, D, E]` where `C` fails:

| Strategy | Restarts |
| --- | --- |
| `one_for_one` | `C` |
| `one_for_all` | `A B C D E` |
| `rest_for_one` | `C D E` |

Pick `one_for_one` when children are independent, `one_for_all` when they share state that a partial restart would leave inconsistent.

```bash
gust check src/upload.gu
gust build src/upload.gu
```

## What you get

The channel becomes a struct wrapping a Tokio channel:

```rust
pub struct UploadsChannel { /* ... */ }

impl UploadsChannel {
    pub fn new() -> Self { /* ... */ }
    pub fn sender(&self) -> tokio::sync::mpsc::Sender<Photo> { /* ... */ }
    pub fn try_send(&self, msg: Photo) { /* ... */ }
    pub async fn receive(&self) -> Option<Photo> { /* ... */ }
}
```

And `spawn` becomes a call into the runtime's supervisor, threaded in as an extra argument:

```rust
pub fn start(
    &mut self,
    worker_count: i64,
    supervisor: &gust_runtime::prelude::SupervisorRuntime,
) -> Result<(), IntakeError>
```

::: callout info "Codegen gives you the skeleton, not the worker loop"
The body Gust spawns for a child is empty. It registers a named child with the supervisor so restart accounting is correct, and leaves the actual work to you — because "what a worker does with a message" is application code, not something the state graph can know. You write that loop below.
:::

## Write the worker loop

```rust "src/main.rs"
#![allow(clippy::new_without_default)]

use std::sync::Arc;

include!("upload.g.rs");

struct LiveEffects;

impl UploadEffects for LiveEffects {
    fn scan_photo(&self, photo: &Photo) -> bool {
        !photo.id.starts_with("quarantine-")
    }

    async fn store_photo(&self, photo: &Photo) -> String {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        format!("https://cdn.example.com/{}.jpg", photo.id)
    }

    fn notify_uploader(&self, photo_id: &str, url: &str) {
        println!("  [email] {photo_id} is live at {url}");
    }
}

/// Pulls photos off the channel and runs each one through its own machine.
async fn run_worker(channel: Arc<UploadsChannel>) -> Result<(), String> {
    let effects = LiveEffects;
    while let Some(photo) = channel.receive().await {
        let mut upload = Upload::new();
        upload.receive(photo).map_err(|err| err.to_string())?;
        upload.scan(&effects).map_err(|err| err.to_string())?;

        // scan lands in Scanned or Rejected; only one of those can publish.
        if matches!(upload.state(), UploadState::Scanned { .. }) {
            upload
                .publish(&effects)
                .await
                .map_err(|err| err.to_string())?;
        }
        println!("  worker finished at {:?}", upload.state());
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    let channel = Arc::new(UploadsChannel::new());
    let supervisor = SupervisorRuntime::with_strategy(RestartStrategy::OneForOne);

    let mut intake = Intake::new();
    intake
        .start(1, &supervisor)
        .expect("start is legal from Booting");
    println!("intake: {:?}", intake.state());

    let worker = tokio::spawn(run_worker(Arc::clone(&channel)));

    channel.try_send(Photo {
        id: "img-0001".to_string(),
        bytes: 248_000,
    });
    channel.try_send(Photo {
        id: "quarantine-9".to_string(),
        bytes: 1_000,
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    intake.drain().expect("drain is legal from Serving");
    println!("intake: {:?}", intake.state());

    worker.abort();
}
```

`SupervisorRuntime` and `RestartStrategy` come from the Gust runtime prelude, which the generated file already imports. The strategy you name in the `.gu` documents the intent; the strategy the runtime actually applies is the one you construct here, so keep the two in step.

```bash
cargo run
```

```text
intake: Serving { workers: 1 }
  [email] img-0001 is live at https://cdn.example.com/img-0001.jpg
  worker finished at Published { photo: Photo { id: "img-0001", bytes: 248000 }, url: "https://cdn.example.com/img-0001.jpg" }
  worker finished at Rejected { photo: Photo { id: "quarantine-9", bytes: 1000 }, reason: "failed the malware scan" }
intake: Draining { workers: 1 }
```

Two photos through one queue, one published and one rejected, with the supervisor tracking the worker and `Intake` recording its own lifecycle as ordinary states.

## Two rough edges

::: callout warning "`sends` on a machine does not compile yet"
Annotating a machine with `sends SomeChannel` emits its send helper outside the `impl` block, so the generated Rust fails with ``self` parameter is only allowed in associated functions`. Until that is fixed, drive the producing side from your own code — `channel.try_send(..)` or `channel.sender()` — as `main` does above. `receives` is unaffected.
:::

The generated `UploadsChannel::new()` also has no `Default` impl, which trips `clippy::new_without_default` under `-D warnings`. That is what the `#![allow(clippy::new_without_default)]` at the top of `main.rs` is for. Add the same line to the top of `tests/upload.rs`, which includes the generated file too — otherwise `cargo clippy --all-targets` fails on the test binary rather than the main one, which is a confusing place to look.

## Where supervision ends

Be clear about the division of labour, because it is easy to expect more than is there. Gust generates the state machines, the channel type, and the supervisor wiring. It does not generate the worker loop, does not run a scheduler, and does not restart anything on its own — `SupervisorRuntime` tracks children and tells you which ones a strategy says to restart, and your code acts on that.

That is deliberate. The machine stays a description of behaviour you can read, diagram, and test; the concurrency is ordinary Rust you can reason about with ordinary Rust tools.

One thing left: getting this into a build. [Deployment](./deployment.md).
