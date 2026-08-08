---
title: "Going Async"
description: "Make the CDN upload an async effect, drive the machine from Tokio, and bound a slow transition with a timeout."
type: tutorial
---

# Going Async

`store_photo` pretends to upload a file. A real one talks to a CDN over the network, which means it has to be `async`. On this page you make that change and follow it through the generated code, your binary, and your tests.

## Mark the effect and its handler

Two keywords. Add `async` to the effect declaration, and to the handler that performs it.

```gust
type Photo {
    id: String,
    bytes: i64,
}

machine Upload {
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

    on scan(ctx) {
        if ctx.photo.bytes > 8000000 {
            goto Rejected(ctx.photo, "larger than the 8 MB limit");
        } else if perform scan_photo(ctx.photo) {
            goto Scanned(ctx.photo);
        } else {
            goto Rejected(ctx.photo, "failed the malware scan");
        }
    }

    async on publish(ctx) {
        let url = perform store_photo(ctx.photo);
        perform notify_uploader(ctx.photo.id, url);
        goto Published(ctx.photo, url);
    }

    on discard() {
        goto Waiting;
    }
}
```

The rule is simple: **a handler that performs an async effect must itself be async.** `scan` stays synchronous because `scan_photo` is; `publish` becomes async because `store_photo` is.

Regenerate:

```bash
gust check src/upload.gu
gust build src/upload.gu
```

## What changed in the generated code

The trait method is no longer written as `async fn`:

```rust
pub trait UploadEffects {
    fn scan_photo(&self, photo: &Photo) -> bool;
    fn store_photo(&self, photo: &Photo) -> impl ::core::future::Future<Output = String> + Send;
    fn notify_uploader(&self, photo_id: &str, url: &str);
}
```

That shape is deliberate. `async fn` in a public trait makes no promise that the returned future is `Send`, and anything holding the machine across an `.await` on a multi-threaded runtime needs exactly that promise. Spelling the future out in return position lets Gust add the `+ Send` bound.

You still write a plain `async fn` in the implementation — Rust matches the two up for you.

The transition method became `async` as well:

```rust
pub async fn publish(&mut self, effects: &impl UploadEffects) -> Result<(), UploadError>
```

## Add Tokio and update the binary

Gust does not choose a runtime for you; the generated Rust is runtime-agnostic. Bring in Tokio yourself.

```toml "Cargo.toml"
[package]
name = "photo-pipeline"
version = "0.1.0"
edition = "2021"

[dependencies]
gust-runtime = "0.3"
serde = { version = "1", features = ["derive"] }
thiserror = "2"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "time"] }
```

```rust "src/main.rs"
include!("upload.g.rs");

struct LiveEffects;

impl UploadEffects for LiveEffects {
    fn scan_photo(&self, photo: &Photo) -> bool {
        !photo.id.starts_with("quarantine-")
    }

    async fn store_photo(&self, photo: &Photo) -> String {
        // Stand-in for the CDN round trip.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        format!("https://cdn.example.com/{}.jpg", photo.id)
    }

    fn notify_uploader(&self, photo_id: &str, url: &str) {
        println!("  [email] {photo_id} is live at {url}");
    }
}

#[tokio::main]
async fn main() {
    let effects = LiveEffects;
    let mut upload = Upload::new();

    upload
        .receive(Photo {
            id: "img-0001".to_string(),
            bytes: 248_000,
        })
        .expect("receive is legal from Waiting");
    upload.scan(&effects).expect("scan is legal from Received");
    upload
        .publish(&effects)
        .await
        .expect("publish is legal from Scanned");

    println!("final: {:?}", upload.state());
}
```

Only `publish` gains an `.await`. `receive` and `scan` are unchanged, because only the handlers that perform async effects become async — the machine is not async wholesale.

```bash
cargo run
```

```text
  [email] img-0001 is live at https://cdn.example.com/img-0001.jpg
final: Published { photo: Photo { id: "img-0001", bytes: 248000 }, url: "https://cdn.example.com/img-0001.jpg" }
```

## Your test double has to become `Sync`

Run `cargo test` now and the test file from the [previous page](./testing.md) fails to compile:

```text
error: future cannot be sent between threads safely
   |
18 |     async fn store_photo(&self, photo: &Photo) -> String {
   |                                                   ^^^^^^ future returned by `store_photo` is not `Send`
   |
   = help: within `FakeEffects`, the trait `Sync` is not implemented for `RefCell<Vec<String>>`
```

That is the `+ Send` bound doing its job. The future borrows `&FakeEffects`, and a `&T` is only `Send` when `T` is `Sync` — which `RefCell` is not. Swap the recorder for a `Mutex` and add `.await` to the calls:

```rust "tests/upload.rs"
use std::sync::Mutex;

include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/upload.g.rs"));

#[derive(Default)]
struct FakeEffects {
    clean: bool,
    notified: Mutex<Vec<String>>,
}

impl UploadEffects for FakeEffects {
    fn scan_photo(&self, _photo: &Photo) -> bool {
        self.clean
    }

    async fn store_photo(&self, photo: &Photo) -> String {
        format!("https://cdn.test/{}.jpg", photo.id)
    }

    fn notify_uploader(&self, photo_id: &str, _url: &str) {
        self.notified.lock().unwrap().push(photo_id.to_string());
    }
}

fn photo(id: &str, bytes: i64) -> Photo {
    Photo {
        id: id.to_string(),
        bytes,
    }
}

#[tokio::test]
async fn a_clean_photo_reaches_published() {
    let effects = FakeEffects {
        clean: true,
        ..Default::default()
    };
    let mut upload = Upload::new();

    upload.receive(photo("img-1", 1_000)).unwrap();
    upload.scan(&effects).unwrap();
    upload.publish(&effects).await.unwrap();

    let UploadState::Published { url, .. } = upload.state() else {
        panic!("expected Published, got {:?}", upload.state());
    };
    assert_eq!(url, "https://cdn.test/img-1.jpg");
    assert_eq!(effects.notified.lock().unwrap().as_slice(), ["img-1"]);
}
```

Note that the fake `store_photo` does not await anything. It does not have to — an `async fn` that never yields is still a valid future, and a test double that returns instantly is exactly what you want. Apply the same treatment to the other three tests: `#[tokio::test]`, `async fn`, `.await` on `publish`, and `.lock().unwrap()` in place of `.borrow()`.

```bash
cargo test
```

```text
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Bounding a slow transition

A CDN that never answers hangs your handler forever. You can put a ceiling on it in the transition declaration:

```gust
type Photo {
    id: String,
    bytes: i64,
}

machine SlowUpload {
    state Scanned(photo: Photo)
    state Published(photo: Photo, url: String)

    transition publish: Scanned -> Published timeout 10s

    async effect store_photo(photo: Photo) -> String

    async on publish(ctx) {
        let url = perform store_photo(ctx.photo);
        goto Published(ctx.photo, url);
    }
}
```

Units are `ms`, `s`, `m`, and `h`. Codegen wraps the handler body in `tokio::time::timeout`, and on expiry the transition returns `Err(SlowUploadError::Failed { reason: "transition 'publish' timed out after ..." })`.

::: callout warning "A timeout bounds the handler, not the state"
`timeout` is a watchdog on how long the handler may run. It is not a deadline on how long the machine may sit in a state, and there is no timeout target state — on expiry the machine stays exactly where it was and the caller gets an `Err`. Adding `| TimedOut` to the transition gains you nothing, because that path is never reached.

For "expire this upload if nobody publishes it within an hour", stamp the entry time into the state and poll a self-transition that compares it against a `current_time_ms()` effect. Gust runs no background clock; that polling is yours to drive.
:::

One side effect worth knowing: adding a `timeout` makes the generated transition method `async` even when the handler is synchronous.

Your pipeline now does real work over the network. Next you put several of them behind a queue: [Supervision](./supervision.md).
