---
title: "Adding Effects"
description: "Declare the pipeline's side effects in Gust and implement them as a Rust trait."
type: tutorial
---

# Adding Effects

Your machine can move between states but it cannot *do* anything: no virus scan, no upload, no email. On this page you declare those three operations in the `.gu` and implement them in Rust.

## Why effects are declarations

Gust's expression language is deliberately tiny. There are no method calls, no struct literals, no loops. You cannot write `photo.bytes.checked_mul(2)` or call a library — and that is the point, because it keeps the machine's behaviour analysable and lets the same source compile to both Rust and Go.

Anything the machine needs from the outside world is therefore *declared* rather than written. An `effect` is a signature; the host language supplies the body.

## Declare the effects

Add three declarations to `machine Upload` in `src/upload.gu`, and rewrite `scan` and `publish` to use them.

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
    effect store_photo(photo: Photo) -> String

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

    on publish(ctx) {
        let url = perform store_photo(ctx.photo);
        perform notify_uploader(ctx.photo.id, url);
        goto Published(ctx.photo, url);
    }

    on discard() {
        goto Waiting;
    }
}
```

Four details:

- **The return type is mandatory.** `effect log(msg: String)` does not parse; write `-> ()` when there is no result, as `notify_uploader` does.
- **`perform` invokes an effect**, and it is an expression as well as a statement. `let url = perform store_photo(ctx.photo);` binds the result; `perform notify_uploader(...)` on its own line runs it and throws the result away; `else if perform scan_photo(ctx.photo)` uses it directly as a condition.
- **`publish` no longer takes a `url` argument.** The URL is now something the machine obtains rather than something the caller supplies, which is a better model of what actually happens.
- **`notify_uploader` is an `action`, not an `effect`.** That distinction is next.

## `effect` versus `action`

The two keywords have identical syntax and generate identical code. The difference is a promise you are making about the operation:

| Keyword | Means | Examples here |
| --- | --- | --- |
| `effect` | Idempotent. Running it twice is harmless. | `scan_photo`, `store_photo` |
| `action` | Externally visible and **not** safe to repeat. | `notify_uploader` |

Sending the uploader two emails because a workflow engine replayed a step is a real bug, so a replay-aware runtime needs to know which calls it must checkpoint before. Marking `notify_uploader` as an `action` is how you tell it.

Because of that, the validator enforces two rules: **at most one action per code path**, and **the action must be the last side-effectful step before the `goto`**. In `publish`, `store_photo` runs first and `notify_uploader` last, which satisfies both.

## What the compiler generates

```bash
gust check src/upload.gu
gust build src/upload.gu
```

`src/upload.g.rs` now contains a trait:

```rust
pub trait UploadEffects {
    /// gust:effect -- replay-safe / idempotent
    fn scan_photo(&self, photo: &Photo) -> bool;
    /// gust:effect -- replay-safe / idempotent
    fn store_photo(&self, photo: &Photo) -> String;
    /// gust:action -- not replay-safe / externally visible
    fn notify_uploader(&self, photo_id: &str, url: &str);
}
```

Note the shapes. Methods take `&self` and borrow their arguments; `String` parameters arrive as `&str`; the `-> ()` return becomes no return type at all; and the `effect` / `action` distinction survives into a doc comment that tooling can read.

The transition methods changed too. Handlers that perform something now take the effects:

```rust
pub fn scan(&mut self, effects: &impl UploadEffects) -> Result<(), UploadError>
```

while `receive` and `discard` — which perform nothing — keep their old signatures. Expect the signatures to differ across transitions of the same machine; it trips people up once.

## Implement the trait

```rust "src/main.rs"
include!("upload.g.rs");

struct LiveEffects;

impl UploadEffects for LiveEffects {
    fn scan_photo(&self, photo: &Photo) -> bool {
        // Stand-in for a real scanner. Anything from the quarantine
        // bucket is treated as infected.
        !photo.id.starts_with("quarantine-")
    }

    fn store_photo(&self, photo: &Photo) -> String {
        format!("https://cdn.example.com/{}.jpg", photo.id)
    }

    fn notify_uploader(&self, photo_id: &str, url: &str) {
        println!("  [email] {photo_id} is live at {url}");
    }
}

fn main() {
    let effects = LiveEffects;
    let mut upload = Upload::new();

    upload
        .receive(Photo {
            id: "img-0001".to_string(),
            bytes: 248_000,
        })
        .expect("receive is legal from Waiting");
    upload.scan(&effects).expect("scan is legal from Received");
    upload.publish(&effects).expect("publish is legal from Scanned");

    println!("final: {:?}", upload.state());

    let mut infected = Upload::new();
    infected
        .receive(Photo {
            id: "quarantine-9".to_string(),
            bytes: 1_000,
        })
        .unwrap();
    infected.scan(&effects).unwrap();
    println!("infected: {:?}", infected.state());
}
```

```bash
cargo run
```

```text
  [email] img-0001 is live at https://cdn.example.com/img-0001.jpg
final: Published { photo: Photo { id: "img-0001", bytes: 248000 }, url: "https://cdn.example.com/img-0001.jpg" }
infected: Rejected { photo: Photo { id: "quarantine-9", bytes: 1000 }, reason: "failed the malware scan" }
```

## Reach for an effect whenever you want a method

You will hit the edge of the expression language quickly — the first time you want `photos.len()`, or to build a `Photo` from parts, or to read the clock. The answer is always the same: declare an effect for it.

```gust
machine Digest {
    state Idle
    state Counted(total: i64, at_ms: i64)

    transition count: Idle -> Counted

    effect photo_count(bucket: String) -> i64
    effect current_time_ms() -> i64

    on count(bucket: String) {
        goto Counted(perform photo_count(bucket), perform current_time_ms());
    }
}
```

This is idiomatic Gust, not a workaround. The judgement call is granularity: every effect is a trait method somebody has to implement, so keep them coarse. One `compute_backoff` effect beats four effects for multiply, min, random, and clamp.

The trait is about to pay for itself. On the [next page](./testing.md) you implement it a second time and test the whole pipeline without a network.
