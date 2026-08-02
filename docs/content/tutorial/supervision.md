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

A `supervises` clause also generates a contract: one runner per child, plus a
table naming each child's restart strategy.

```rust
pub trait IntakeSupervision {
    fn run_upload(&self, child: Upload)
        -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;
}

pub const INTAKE_SUPERVISION: &[(&str, RestartStrategy)] =
    &[("Upload", RestartStrategy::OneForOne)];
```

And `spawn` takes both the supervisor and your runners:

```rust
pub fn start(
    &mut self,
    worker_count: i64,
    supervisor: &gust_runtime::prelude::SupervisorRuntime,
    children: &impl IntakeSupervision,
) -> Result<(), IntakeError>
```

::: callout info "Codegen builds the child, you drive it"
`spawn Upload()` constructs a real `Upload` and hands it to `run_upload`. What it
does not do is run it — a machine is passive, its transitions are called from
outside, so there is no loop for Gust to hand the supervisor. "What a worker does
with a message" is application code, not something the state graph can know. You
write that loop below, exactly as you write the effects.
:::

## Write the worker loop

```rust "src/main.rs"
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

/// Pulls photos off the channel and runs one machine per photo.
///
/// `Intake` hands us a freshly constructed `Upload` per spawn; this loop is what
/// gives it something to do.
struct Workers {
    channel: Arc<UploadsChannel>,
}

impl IntakeSupervision for Workers {
    fn run_upload(
        &self,
        child: Upload,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>> {
        let channel = Arc::clone(&self.channel);
        Box::pin(run_worker(child, channel))
    }
}

async fn run_worker(mut upload: Upload, channel: Arc<UploadsChannel>) -> Result<(), String> {
    let effects = LiveEffects;
    while let Some(photo) = channel.receive().await {
        upload = Upload::new();
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

    let workers = Workers {
        channel: Arc::clone(&channel),
    };

    let mut intake = Intake::new();
    intake
        .start(1, &supervisor, &workers)
        .expect("start is legal from Booting");
    println!("intake: {:?}", intake.state());

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
}
```

`SupervisorRuntime` and `RestartStrategy` come from the Gust runtime prelude, which the generated file already imports. The strategy you name in the `.gu` reaches the output as `INTAKE_SUPERVISION`; the strategy the runtime *applies* is the one you construct here, so either read the table when constructing the runtime or keep the two in step by hand.

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

## Where supervision ends

Be clear about the division of labour, because it is easy to expect more than is
there.

Gust generates the state machines, the channel type, and the supervision
*contract*: a `{Machine}Supervision` trait with one runner per child, and a table
naming each child's restart strategy. `spawn Worker(cfg)` constructs a real
`Worker` and hands it to your runner.

It does not generate the worker loop, does not run a scheduler, and does not
restart anything on its own. A machine is passive — its transitions are called
from outside — so there is no loop for Gust to hand the supervisor. You write
that, exactly as you write the effects. `SupervisorRuntime` tracks children and
tells you which ones a strategy says to restart; your code acts on it.

That split is deliberate and it is the same one effects use: Gust owns what can
be checked, your code owns what must run.

That is deliberate. The machine stays a description of behaviour you can read, diagram, and test; the concurrency is ordinary Rust you can reason about with ordinary Rust tools.

One thing left: getting this into a build. [Deployment](./deployment.md).
