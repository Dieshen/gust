---
title: "Testing"
description: "Implement a second, deterministic UploadEffects and test every branch of the pipeline without touching the network."
type: tutorial
---

# Testing

`LiveEffects` uploads to a CDN and sends email. You do not want either of those in a test suite — and you do not have to have them, because the machine never calls a CDN. It calls `UploadEffects`.

That is the practical payoff of declaring side effects instead of inlining them. Every transition method takes `effects: &impl UploadEffects`, so a test supplies a different implementation and the machine cannot tell the difference. No mocking framework, no dependency injection container, no trait objects: just a second `impl`.

## Write the test double

Create `tests/upload.rs`. It pulls the generated file in the same way `main.rs` does, with an absolute path so the test binary finds it.

```rust "tests/upload.rs"
use std::cell::RefCell;

include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/upload.g.rs"));

#[derive(Default)]
struct FakeEffects {
    /// What the scanner should report for every photo in this test.
    clean: bool,
    /// Every photo id `notify_uploader` was called with, in order.
    notified: RefCell<Vec<String>>,
}

impl UploadEffects for FakeEffects {
    fn scan_photo(&self, _photo: &Photo) -> bool {
        self.clean
    }

    fn store_photo(&self, photo: &Photo) -> String {
        format!("https://cdn.test/{}.jpg", photo.id)
    }

    fn notify_uploader(&self, photo_id: &str, _url: &str) {
        self.notified.borrow_mut().push(photo_id.to_string());
    }
}

fn photo(id: &str, bytes: i64) -> Photo {
    Photo {
        id: id.to_string(),
        bytes,
    }
}
```

`FakeEffects` does two jobs at once, and both matter.

It **controls** what the machine sees: `clean` decides the scanner's verdict, so a test can force either branch of `scan` without needing an actual infected file. And it **records** what the machine did: `notified` captures every call to the action, so a test can assert that the email was sent — or, more usefully, that it was not.

The effects methods take `&self`, not `&mut self`, which is why the recorder is wrapped in a `RefCell`.

## Test the happy path

```rust "tests/upload.rs"
#[test]
fn a_clean_photo_reaches_published() {
    let effects = FakeEffects {
        clean: true,
        ..Default::default()
    };
    let mut upload = Upload::new();

    upload.receive(photo("img-1", 1_000)).unwrap();
    upload.scan(&effects).unwrap();
    upload.publish(&effects).unwrap();

    let UploadState::Published { url, .. } = upload.state() else {
        panic!("expected Published, got {:?}", upload.state());
    };
    assert_eq!(url, "https://cdn.test/img-1.jpg");
    assert_eq!(effects.notified.borrow().as_slice(), ["img-1"]);
}
```

Two assertions, on two different things. The first says the machine ended in the right state carrying the right data; the second says the uploader was emailed exactly once.

## Test the branches you cannot easily provoke

Flipping `clean` to `false` exercises the infected path — no malware sample required.

```rust "tests/upload.rs"
#[test]
fn an_infected_photo_is_rejected() {
    let effects = FakeEffects {
        clean: false,
        ..Default::default()
    };
    let mut upload = Upload::new();

    upload.receive(photo("img-2", 1_000)).unwrap();
    upload.scan(&effects).unwrap();

    let UploadState::Rejected { reason, .. } = upload.state() else {
        panic!("expected Rejected");
    };
    assert_eq!(reason, "failed the malware scan");
    assert!(effects.notified.borrow().is_empty());
}

#[test]
fn an_oversize_photo_never_reaches_the_scanner() {
    let effects = FakeEffects {
        clean: true,
        ..Default::default()
    };
    let mut upload = Upload::new();

    upload.receive(photo("img-3", 12_000_000)).unwrap();
    upload.scan(&effects).unwrap();

    let UploadState::Rejected { reason, .. } = upload.state() else {
        panic!("expected Rejected");
    };
    assert_eq!(reason, "larger than the 8 MB limit");
}
```

The `assert!(effects.notified.borrow().is_empty())` in the first test is the one to keep. Asserting that a rejected photo produced **no** email is the kind of check that is awkward when the email goes through a real client and trivial when it goes through a trait.

## Test that illegal moves stay illegal

```rust "tests/upload.rs"
#[test]
fn publishing_before_scanning_is_an_error() {
    let effects = FakeEffects {
        clean: true,
        ..Default::default()
    };
    let mut upload = Upload::new();

    upload.receive(photo("img-4", 1_000)).unwrap();
    let err = upload.publish(&effects).unwrap_err();

    assert!(matches!(err, UploadError::InvalidTransition { .. }));
    assert!(effects.notified.borrow().is_empty());
}
```

This one is a regression test on the state graph itself. If somebody later adds `transition publish: Scanned | Received -> Published` to widen the machine, this test fails and asks them to justify it.

## Run them

```bash
cargo test
```

```text
running 4 tests
test an_oversize_photo_never_reaches_the_scanner ... ok
test a_clean_photo_reaches_published ... ok
test publishing_before_scanning_is_an_error ... ok
test an_infected_photo_is_rejected ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Four tests, no network, no clock, no filesystem, and full coverage of every branch in the machine.

## What to test, and what not to

You do not need to test that `receive` moves `Waiting` to `Received`. The generated code does that by construction, and a test asserting it only re-states the `.gu`.

What is worth testing is everything the `.gu` does *not* say on its own:

- **Branch conditions** — that 8 MB is the threshold and that the comparison is the right way round.
- **The data carried forward** — that the URL in `Published` is the one `store_photo` returned, not a stale one.
- **Effects that must not fire** — no email on the rejection paths.
- **Illegal transitions** — the guarantees you are relying on the state graph to enforce.

Next, one of the effects becomes a real network call: [Going Async](./async.md).
