---
title: "Getting Started"
description: "Install the Gust compiler, write a two-state machine, generate Rust from it, and run it."
type: tutorial
---

# Getting Started

By the end of this page you will have a Rust program that drives a Gust state machine and prints its state. It is about forty lines in total, and every one of them is on this page.

## Install the compiler

The compiler ships as the `gust-cli` crate; the binary it installs is called `gust`.

```bash
cargo install gust-cli --locked
```

Check it landed:

```bash
gust --version
```

You should see `gust 0.3.0` or newer. If `gust` is not found, `~/.cargo/bin` is missing from your `PATH`.

## Create the project

Gust does not replace Cargo — it generates Rust that a normal Cargo crate compiles. So start with a normal binary crate.

```bash
cargo new photo-pipeline
cd photo-pipeline
```

Generated Gust code is not self-contained. It derives `Serialize` and `Deserialize`, derives `thiserror::Error` for the machine's error type, and imports the Gust runtime prelude. All three crates have to be direct dependencies.

```toml "Cargo.toml"
[package]
name = "photo-pipeline"
version = "0.1.0"
edition = "2021"

[dependencies]
gust-runtime = "0.3"
serde = { version = "1", features = ["derive"] }
thiserror = "2"
```

::: callout warning "Three dependencies, not one"
`gust-runtime`'s prelude re-exports `serde` and `thiserror`, which makes it look as though the one dependency is enough. It is not. Generated code writes `use serde::{Serialize, Deserialize};` and `#[derive(thiserror::Error)]` — direct paths that a re-export through another crate cannot satisfy. Omitting either produces a page of errors in a file you did not write. This is the most common way a first Gust integration fails.
:::

## Write the machine

Gust source lives in `.gu` files. Save this one as `src/upload.gu` — it models a photo arriving and being accepted for processing.

```gust
type Photo {
    id: String,
    bytes: i64,
}

machine Upload {
    state Waiting
    state Received(photo: Photo)

    transition receive: Waiting -> Received

    on receive(photo: Photo) {
        goto Received(photo);
    }
}
```

Read it in order:

- `type` declares a struct. Gust uses `type`, not `struct`.
- `state` names a condition the machine can be in. States may carry fields; `Waiting` carries none, `Received` carries the photo.
- `transition` names an event and says which state it moves from and to. The first state declared is the starting state.
- `on` is the handler that runs when the transition fires. Every path through it ends in `goto`, which sets the new state.

## Check it

Before generating anything, ask the compiler whether the source is valid:

```bash
gust check src/upload.gu
```

```text
Check passed
```

`gust check` writes nothing. Run it whenever you edit a `.gu` — it is the fastest signal you have.

## Generate the Rust

```bash
gust build src/upload.gu
```

```text
Generated .../photo-pipeline/src/upload.g.rs
```

Open `src/upload.g.rs` and look, but do not edit it. The `.g.rs` extension marks it as generated output; the next `gust build` overwrites whatever you put there. Inside you will find a `Photo` struct, an `UploadState` enum with a `Waiting` variant and a `Received { photo }` variant, an `UploadError` enum, and an `Upload` struct with `new()`, `state()`, and `receive()`.

Notice that `state Received(photo: Photo)` became `Received { photo: Photo }` — a named struct variant, taking its field name straight from the `.gu`.

## Wire it into the crate

Codegen writes the file; it does not touch your module tree. Bring it in with `include!`.

```rust "src/main.rs"
include!("upload.g.rs");

fn main() {
    let mut upload = Upload::new();
    println!("start: {:?}", upload.state());

    let photo = Photo {
        id: "img-0001".to_string(),
        bytes: 248_000,
    };

    upload
        .receive(photo)
        .expect("receive is legal from Waiting");
    println!("after receive: {:?}", upload.state());

    let err = upload
        .receive(Photo {
            id: "img-0002".to_string(),
            bytes: 1,
        })
        .expect_err("receive is not legal twice");
    println!("rejected: {err}");
}
```

::: callout warning "Use `include!`, not `#[path] mod`"
`rustfmt` follows `mod` declarations, so wiring a generated file in with `#[path = "upload.g.rs"] mod upload;` lets `cargo fmt` quietly rewrite it. The file then no longer matches what `gust build` emits, and every staleness check in your CI starts failing over a file nobody meaningfully changed. `rustfmt` does not follow `include!`.
:::

## Run it

```bash
cargo run
```

```text
start: Waiting
after receive: Received { photo: Photo { id: "img-0001", bytes: 248000 } }
rejected: invalid transition 'receive' from state 'Received { photo: Photo { id: "img-0001", bytes: 248000 } }'
```

The third line is the interesting one. `receive` is declared as `Waiting -> Received`, so calling it from `Received` is not a legal move — and the generated method tells you so by returning `Err` rather than panicking or quietly doing the wrong thing.

That is the whole idea, in three lines of output. Next you will grow this two-state stub into the full upload lifecycle: [Your First Machine](./first_machine.md).
