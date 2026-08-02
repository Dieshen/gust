---
title: "Security"
description: "The trust boundaries around Gust: why gust generate confines its output paths, which parts of the toolchain are not confined, and what a machine does and does not enforce at runtime."
type: guide
---

# Security

Gust's security surface is small, because the compiler does not execute your machines and the generated code does not perform I/O. What it does have is one place where untrusted input reaches the filesystem, and several places where the type-safety guarantee is narrower than it first appears.

This guide is about both.

## A `gust.toml` is untrusted input

Cloning a repository and running its build is ordinary. So is cloning a repository that contains a `gust.toml` and running `gust generate` inside it. That makes the manifest attacker-controlled input in exactly the way a `Makefile` is — except that a manifest looks like configuration, which invites less suspicion.

The specific hazard is the `output` key. A manifest target declares where generated files land:

```toml
[targets.rust]
output = "src/generated"
```

Nothing about that syntax stops it being `../../../../.ssh` or an absolute path. Before this was closed, a manifest could write generated files anywhere the invoking user could write.

**`gust generate` now confines output paths.** A target's `output` must resolve beneath one of two roots:

- **the manifest directory** — so `gust generate --config /elsewhere/gust.toml` works when run from an unrelated working directory;
- **the current directory** — so the documented layout, where the manifest sits in a subdirectory and emits into sibling projects (`output = "../rs-project/src/generated"`), still works.

Both roots are chosen by *you*, on the command line. Neither is chosen by the manifest. A path escaping both is refused:

```text
error: target output '../../../../../pwned' resolves to '/tmp/pwned', which is
outside both the manifest directory '/tmp/scratch/proj' and the current
directory '/tmp/scratch/proj'.
Pass --allow-outside if this is intended.
```

The check runs **after** normalising `.` and `..`, so an escape buried mid-path (`src/../../../../etc`) is refused too, not just a leading one. A `..` that would escape the root is deliberately retained rather than popped, so an escaping path can never normalise into something that looks contained.

This affects 0.3.0 and was fixed in 0.4.0.

### `--allow-outside`

The restriction is an opt-out, not a wall:

```bash
gust generate --allow-outside
```

That is the right shape for a build tool — there are legitimate layouts that write outside both roots — but it means the flag is the security boundary. Treat it accordingly:

- **Never put `--allow-outside` in a CI workflow** that runs against pull requests from forks. A contributor who can edit `gust.toml` then chooses where the runner writes.
- **Do not add it to a shared script** to make an error go away. The error names the resolved path; read it and decide whether that path is one you meant.

### What is *not* confined

`gust build -o <dir>` is unrestricted. It will write outside the working directory without complaint:

```bash
gust build machine.gu -o ../../anywhere    # succeeds
```

This is deliberate and correct. The distinction is *who chose the path*. With `gust build` you typed the destination on the command line; with `gust generate` the destination came out of a file that arrived with the repository. Confining the first would be an obstacle with no threat behind it.

The normalisation is also purely **lexical** — it does not touch the filesystem and does not resolve symlinks. A symlink inside the permitted tree that points elsewhere is still followed when the file is written. Lexical checking is the conventional trade: it avoids a time-of-check/time-of-use race and works on paths that do not exist yet. If you are generating into a directory tree you do not control, that residual is yours to think about.

## Generated files are code that reviewers skim

A `.g.rs` is hundreds of lines of machine-written Rust with a banner saying not to edit it. That is precisely the kind of file that gets scrolled past in a pull request, which makes a committed generated file an attractive place to hide a change.

The mitigation is mechanical. If you commit generated output, verify in CI that it still matches its source:

```bash
gust generate --check
```

It regenerates in memory and fails if any committed file differs, without writing anything. A generated file that no longer corresponds to its `.gu` becomes a build failure rather than a diff nobody read.

::: callout warning "Do not rely on modification times for this"
`gust-build` decides whether to regenerate by comparing mtimes. After a fresh clone every file shares one checkout timestamp, so a stale — or tampered — generated file is never rewritten and never noticed. `gust generate --check` compares content. Use it.
:::

The same reasoning applies to letting *tools* rewrite generated files. `cargo fmt` follows `mod` declarations, so wiring a `.g.rs` in with `#[path] mod` lets rustfmt silently reformat it, after which it no longer matches what the compiler emits. Prefer `include!`, which rustfmt does not follow.

## What a machine enforces at runtime

The type-safety guarantee is real but narrow, and it is worth knowing exactly where it stops.

### Transitions fail closed

Calling a transition that is not legal from the current state returns `Err`. It does not panic, and it does not silently do nothing:

```gust
machine Withdrawal {
    state Requested(account: String, amount_cents: i64)
    state Approved(account: String, amount_cents: i64)
    state Settled(reference: String)
    state Rejected(account: String, reason: String)

    transition approve: Requested -> Approved | Rejected
    transition settle: Approved -> Settled

    effect check_limits(account: String, amount_cents: i64) -> bool
    action move_funds(account: String, amount_cents: i64) -> String

    on approve(ctx: ApproveCtx) {
        let allowed = perform check_limits(ctx.account, ctx.amount_cents);
        if allowed {
            goto Approved(ctx.account, ctx.amount_cents);
        } else {
            goto Rejected(ctx.account, "limit exceeded");
        }
    }

    on settle(ctx: SettleCtx) {
        let reference = perform move_funds(ctx.account, ctx.amount_cents);
        goto Settled(reference);
    }
}
```

Calling `settle()` on a machine still in `Requested` returns `Err(WithdrawalError::InvalidTransition { … })`. Funds are never moved, because the handler body never runs. This is the property worth leaning on when the event stream driving a machine comes from outside your system: **out-of-order and replayed events are rejected by the state graph rather than by your handler code.**

### The state field is public

The guarantee applies to the transition methods, not to the struct. Generated Rust emits:

```rust
pub struct Withdrawal {
    pub state: WithdrawalState,
}
```

That field is public and writable. Nothing stops code in the consuming crate from writing `withdrawal.state = WithdrawalState::Approved { … }` and skipping the approval handler entirely. The Go machine has the same shape, with exported `State` and `*Data` fields.

This is a legitimate escape hatch — it is how you rehydrate a machine — but it means the state graph is a guarantee about *your own crate's discipline*, not an invariant enforced against all code that can see the type. Keep the machine private to the module that owns it and expose the transitions.

### Rehydrating does not validate

The machine derives `Serialize` and `Deserialize`, and the Go backend emits `ToJSON` / `FromJSON`. Neither performs any validation on the way in. Deserialising a checkpoint materialises whatever state the document names, with whatever field values it carries. There is no check that the state is reachable, that its fields are plausible, or that the document was produced by this version of the contract.

So: **treat checkpoint storage as trusted, or validate after loading.** If a machine can be rehydrated from data that crossed a trust boundary, the state it lands in is entirely attacker-chosen — which, for the machine above, means arriving in `Approved` without ever passing `check_limits`.

### Effects are the whole attack surface

A Gust machine performs no I/O. It calls out to the world exclusively through the effect trait you implement, which means every input validation, every authorisation check, and every injection defence lives in your effects implementation — not in the `.gu`.

The `.gu` is worth reading as a security document for a different reason: it is an exhaustive, short list of every way this component touches the outside world. There is no hidden call. If `move_funds` is the only `action`, then moving funds is the only irreversible thing this machine can do.

## Actions, replay, and doing things twice

`action` marks a step as externally visible and *not* safe to replay. The validator enforces two rules: at most one action per code path, and the action must be the last side-effectful step before the `goto`.

Those rules exist so a replay-aware runtime can checkpoint immediately before the action. Anything after it would run twice on resume.

Two things to be clear about:

- **The rules are warnings, not errors.** A `.gu` that violates them still compiles. If you are building a runtime that consumes these contracts, treat the warnings as blocking at import time — a handler that violates them cannot be checkpointed cleanly.
- **The guarantee is at-least-once, not exactly-once.** A crash between executing the action and durably writing its checkpoint means the action runs again on resume. Gust records intent; it cannot make a remote call idempotent. Anything genuinely irreversible needs an idempotency key that the effect implementation passes to the downstream service.

See [Workflow Runtime Integration](workflow_runtime.md#effect-vs-action-replay-semantics) for what a runtime has to do with these signals.

## A short checklist

- `gust generate --check` runs in CI if you commit generated output.
- `--allow-outside` appears in no CI workflow and no shared script.
- Generated files are brought in with `include!`, not `#[path] mod`.
- The machine type is private to its module; callers go through transitions.
- Anything that rehydrates a machine from outside your trust boundary validates the resulting state before driving it.
- Every irreversible `action` is idempotent at the remote end, or carries an idempotency key.

## Where to go next

- [Debugging](debugging.md) — reading validator output, including the action-safety warnings.
- [Workflow Runtime Integration](workflow_runtime.md) — checkpointing and replay in detail.
- [Contract Packages](../advanced/contract_packages.md) — the `gust.toml` manifest schema.
