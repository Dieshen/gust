---
title: "Upgrading 0.3.0 → 0.4.1"
description: "Every breaking change between Gust 0.3.0 and 0.4.1, in the order likely to hit you."
type: guide
---

# Upgrading 0.3.0 → 0.4.1

```bash
cargo install gust-cli --locked --force   # gust 0.4.1
```

Bump `gust-runtime` to `0.4.1` in any crate consuming generated Rust. Regenerate
all `.g.rs` / `.g.go` — do not mix output from two compiler versions in one build.

Ordered by how likely each is to hit you.

---

## 1. `goto` now ends the handler — this changes runtime behaviour

**The one to read.** In 0.3.0, `goto` emitted a bare state assignment and
execution continued:

```rust
if index >= effects.len(&items) {
    self.state = MState::Done { items };   // no return
}
let _ = effects.get(&items, index);        // still ran
```

An early `goto` inside an `if` fell through. The machine ended in whichever
state the **last** assignment named, and any value moved into the abandoned
state left the rest of the handler using a moved value.

`goto` now returns.

| Handler: `goto Early` when `n > 0`, else falls through to `Late` | 0.3.0 | 0.4.x |
|---|---|---|
| `n = 5` | `Late` | `Early` |
| `n = -1` | `Late` | `Late` |

**What to check:** any handler with a `goto` that is *not* the last statement.
If it was written knowing the following statements still ran, they no longer do.
If it was written expecting the `goto` to win, it was silently broken and is now
correct.

This is why `gust-stdlib/saga.gu` never compiled as Rust.

## 2. `supervises` / `spawn` now generate a contract you must implement

In 0.3.0 `supervises` emitted nothing on the Rust backend, and `spawn` emitted a
future that discarded its arguments and returned `Ok(())`. No child was ever
constructed — the feature compiled, ran, and did nothing on every target.

Each machine with a `supervises` clause now emits:

- a runner trait — `{Machine}Supervision` (Rust), `{Machine}Children` (Go) —
  with one method per child;
- a restart-strategy table (`{MACHINE}_SUPERVISION`) naming each child and its
  policy, so a host can drive `SupervisorRuntime::restart_scope` without
  re-parsing the `.gu`.

`spawn Worker(cfg)` builds a real `Worker::new(cfg)` and hands it to that method.

**What to do:** implement the new trait. Gust constructs children because that
is contract; it does not drive them, because a machine is passive — its
transitions are called from outside, so there is no generated loop to hand the
supervisor.

**Arity is now checked** (0.4.1). A machine's constructor comes from the fields
of its **first state**, and that is the arity a `spawn` argument list must match:

```gust
machine Worker {
    state Waiting
    state Busy(job: String)

    transition start: Waiting -> Busy

    on start(job: String) {
        goto Busy(job);
    }
}

machine Boss(supervises Worker(one_for_one)) {
    state Idle(job: String)
    state Running(current: String)

    transition begin: Idle -> Running

    on begin(ctx) {
        spawn Worker();
        goto Running(ctx.job);
    }
}
```

`Worker`'s first state is `Waiting`, which has no fields — so `Worker::new()`
takes nothing and `spawn Worker()` is the only correct form. Writing
`spawn Worker(ctx.job)` there is an error in 0.4.1:

```text
error: spawn of 'Worker' passes 1 argument, but its constructor takes 0
  = note: a machine is constructed from the fields of its first state;
          'Worker' declares 0 there
```

In 0.4.0 that same source reported "Check passed" and then emitted
`Worker::new(job)` against `fn new() -> Self` — `E0061` from `rustc`, pointing
at a generated file you are told never to edit. **If you author `.gu` against
0.4.0, upgrade before trusting `gust check`.**

## 3. `gust build` now validates

`gust build`, `gust watch`, and `gust generate` run the validator. A source the
validator rejects produces no output and exits non-zero.

In 0.3.0 only `gust check` validated, so `gust build` on a program with a `goto`
to a nonexistent state exited 0 and wrote a file calling a method that does not
exist — surfacing as a host-compiler error pointing at generated source.

**What to expect:** a `.gu` that built before may now fail, and an invalid source
sitting in CI appears as a new failure with nothing having changed. Run
`gust check` to see the same diagnostics. Warnings print but do not block.
There is deliberately no `--no-validate`.

## 4. Generic machines: signatures changed

Two fixes, both changing generated Rust for generic machines:

- **A restored parameter.** ctx detection treats an unrecognised type name as
  the marker for the from-state accessor, so `on put(value: T)` on a
  `machine Box<T>` had `value` **dropped from the generated signature**. It is
  back — the method now takes an argument it did not before.
- **A `Debug` bound.** The invalid-transition arm formats state with `{:?}`, and
  a generic state enum's derived `Debug` only applies when its parameters are
  `Debug`. The transition impl now carries that bound, so your `T` must be
  `Debug`.

Non-generic output is unchanged. A machine type parameter that is never used is
now a validator error.

## 5. Go: synchronous `Result` effects changed arity

An effect declared `-> Result<T, E>` now lowers to `(T, error)` whether or not
it is `async`. In 0.3.0 a *synchronous* one returned bare `T` and erased the
failure entirely.

**Update your effect implementations** — the interface method signature changed.
`E` is erased to Go's `error`; when `E` is `String` the `Err` binding receives
`err.Error()`. The validator now **warns** when a non-`String` `E` cannot survive
Go codegen.

## 6. `gust generate` output paths are confined

A manifest's `[targets.*] output` could previously resolve anywhere the invoking
user could write, via `..` or an absolute path. Since cloning a repo and running
`gust generate` inside it is ordinary — and `gust.toml` arrives with the repo —
outputs must now resolve beneath the manifest directory or the working
directory. `--allow-outside` lifts it. Affects 0.3.0.

---

## Smaller behaviour changes

- **`_`-prefixed bindings now warn** like any other unread binding. Gust never
  documented that convention, and Go accepts only a bare `_`, never `_name`.
  Use a bare `perform f();` — the effect still runs.
- **Transitions no longer deep-copy state.** They match on `&self.state` and
  clone only the fields the handler references (`Copy` fields are dereferenced).
  Faster; no source change needed.
- **Shadowed handler parameters warn.** A parameter sharing a name with a
  from-state field is shadowed by the destructure and unreachable.
- **`sends` / `receives` are checked against declared channels.** A typo
  previously made the generated `send_*` helper vanish with no diagnostic.

## Fixes you get for free

No action required, but they change what compiles:

- **Rust:** a machine declaring `sends` now produces code that compiles at all
  (the helper was emitted at module scope with a `&self` receiver). Channels
  were unusable on Rust for any machine with the annotation.
- **Rust:** generated code now passes `clippy -D warnings` — `redundant_field_names`,
  `cmp_owned`, `new_without_default`, `unused_variables`, and a stray
  `use tokio;` all previously broke consumers building with `-D warnings`.
- **Go:** source-state fields readable by bare name; `Ok`/`Err` matching works;
  generic machines compile; `async` effects returning `()` bind one value.
  All seven `gust-stdlib` machines now generate Go that builds — previously
  every one failed.
- **wasm / no_std:** output now compiles. Neither had ever been fed to a
  compiler before this cycle. (Both backends were subsequently **removed in
  1.0** — compiling turned out not to mean implementing the source machine.
  See [Upgrading 0.4 → 1.0](upgrading-0.4-to-1.0.md).)

## Known limits

- **wasm cannot express generics** — `#[wasm_bindgen]` rejects type parameters.
  Backend limit, not an emitter bug. Moot as of 1.0, which removed the backend.
- **Go erases non-`String` `Result` error types** (warned by the validator).
- **`gust-build` does not validate** — unlike the CLI. Use `gust check` in CI.
