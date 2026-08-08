---
title: "Upgrading 0.4 → 1.0"
description: "Every breaking change between Gust 0.4.1 and 1.0, in the order likely to hit you."
type: guide
---

# Upgrading 0.4 → 1.0

```bash
cargo install gust-cli --locked --force   # gust 1.0
```

Bump `gust-runtime` to `1.0` in any crate consuming generated Rust. Regenerate
all `.g.rs` / `.g.go` — do not mix output from two compiler versions in one
build.

1.0 is a [stability promise](stability.md): source that compiles at 1.0 keeps
compiling across 1.x. Getting there meant making the breaking changes now
rather than discovering them later, so this release is deliberately front-loaded.

Ordered by how likely each is to hit you.

::: callout info "This page is written as the changes land"
It is not reconstructed at release time. Two 0.4.0 breaking changes reached
users with no changelog entry at all because the notes were written afterwards;
writing the guide alongside the work is the fix.
:::

---

## 1. The `ctx` parameter loses its type annotation

**Every handler that reads its source state needs a one-line edit.**

```text
on begin(ctx: BeginCtx) {   // 0.4.x
on begin(ctx) {             // 1.0
```

`gust check` tells you exactly this:

```text
error: the 'ctx' parameter must not have a type annotation
  = note: 'ctx' is the from-state accessor: it reads the fields of the state
          this transition leaves, and is dropped from the generated signature.
          It has no type you could name
  = help: write `on begin(ctx)` — before 1.0 this was spelled `ctx: SomeCtx`,
          and that type never existed
```

Nothing else changes: `ctx.field` still resolves against the source state, and
generated output is identical.

### Why this is worth a breaking change

The accessor used to be identified by its **type being unrecognised**. `BeginCtx`
was never declared anywhere — that was the mechanism.

That made *"the compiler does not know this name"* load-bearing syntax, and it
had three consequences:

- **A typo silently deleted a parameter.** `on start(cfg: Confgi)` made `cfg`
  the accessor and dropped it from the generated method. `gust check` could not
  object, because an undeclared type name in that position was the idiom.
- **Generic parameters needed special handling.** A machine's own `T` is not a
  declared type, so `on put(value: T)` on `machine Box<T>` lost its argument
  until the known-type set was threaded through by hand.
- **Growing the type system would have broken untouched source.** Any builtin
  type name added later — or any cross-file import that widened the known set —
  would silently change the signature of handlers that already compiled.

The third is the one that made this a 1.0 item rather than a 1.x cleanup. The
[stability promise](stability.md) says source that compiles at 1.0 keeps
compiling across 1.x, and under the old rule the schema-layer work planned for
1.x could not honour that.

An absent annotation cannot drift as the compiler learns more names.
`BUILTIN_TYPES`, `collect_known_types`, and `machine_known_types` are deleted —
nothing reads them, so the rule cannot come back by accident.

### The rules now

- A parameter with no type annotation must be named `ctx`.
- A parameter named `ctx` must not carry a type.
- Everything else is an ordinary argument: `on go(ctx, retries: i64)` is valid.

### Tooling

`gust fmt` round-trips the new form. The LSP's "add missing handler" code action
and the VS Code snippets emit `on name(ctx)`. The MCP `gust_parse` output now
reports `"type": null` for the accessor, which is how a consumer identifies it.

## 2. `use` no longer emits a host import, and unknown type names are rejected

Two halves of one change: the last places where **"the compiler does not
recognise this name"** carried meaning.

### `use` is a Gust-level import

It had two meanings. `use std::EngineFailure;` was a Gust-virtual stdlib import
that emitted nothing. Any other path — `use crate::domain::OrderId;`, `use os;` —
was passed through as a *host-language* import: a real Rust `use`, a real Go
import.

The second had already stopped working. Handlers may only call declared effects
as of 1.0, and qualified calls with arguments never parsed, so nothing in a `.gu`
could reference an imported symbol. On Go the result was an import unused by
construction, which the compiler rejects outright:

```text
vet: u.g.go:8:2: "os" imported and not used
```

Only the Gust meaning remains: **a `use` names a type declared elsewhere.** The
validator accepts the name; the consuming build is responsible for putting the
declaration in the same module or package. Nothing is emitted.

**What to check:** if a `.gu` carried a `use` in order to reach a host symbol,
that never worked as intended and now emits nothing. Anything a handler needs
from the host is an `effect`.

Reserving the keyword also means it will not change meaning when the module
system arrives in 1.x — which was the point of doing this before 1.0 rather than
after.

### Unknown type names are rejected

A type name must be a primitive, a declared `type`/`enum`, a machine's own
generic parameter, or imported with `use`:

```text
error: unknown type 'Confgi' in field 'cfg' of state 'Start'
  = note: an undeclared type name used to reach the backends verbatim, resolving
          only if the generated file's module or package happened to define it
  = help: did you mean 'Config'?
```

Undeclared names previously passed straight through, so whether a `.gu` compiled
depended on the host's namespace rather than on the `.gu`.

**What to do:** declare the type, or import it. `use` is the escape hatch, and
that is now its whole job.

## 3. Go: `goto` now ends the handler — this changes runtime behaviour

**The one to read if you target Go.** Nothing about your `.gu` changes; what the
generated Go *does* changes.

The Go backend lowered `goto` to a bare state assignment and carried on, so an
early `goto` inside a bare `if` fell through into every later branch:

```go
if verdict.Accept {
    m.State = LifecycleStateAtWax     // assigned...
    m.AtWaxData = &LifecycleAtWaxData{Piece: piece}
}                                     // ...no return
m.State = LifecycleStateScrapped      // ...and immediately overwritten
```

The Rust backend has returned here since 0.4.0; Go was missed, and the
asymmetry survived because nothing tested for it. `gust check` passed, the
output compiled, and `go vet` was quiet — the defect was purely behavioural.

Two failure modes:

| Shape | 0.4.x behaviour | 1.0 |
|---|---|---|
| Fall-through `goto` reads no source-state field | Machine silently lands in the **last** declared target, whatever the condition said | Correct target |
| Fall-through `goto` reads a source-state field | **Nil dereference** — the taken branch already called `clearStateData()` | Correct target |

The first is the more damaging: nothing is raised, no error is returned, and a
multi-target transition effectively collapses to its final target.

**What to check:** any Go-targeting handler with a `goto` that is not the last
statement. If a machine appeared to always reach one particular state, this was
why. Rust-targeting machines are unaffected — they were fixed in 0.4.0.

**A caveat on `timeout` transitions.** A `goto` in a *branch* returns, so it
skips the generated timeout epilogue, which tests the deadline only after the
body has run. That asymmetry is inherited rather than introduced: any early exit
was always going to miss that check. Reworking what a timeout means for a
handler is deliberately out of scope here.

## 4. Handlers may only call declared effects

A bare call in a handler is now a `gust check` error:

```text
error: call to undeclared function 'exit'
  = note: handlers may only call declared effects; Gust has no function
          declarations, so this would be emitted verbatim into generated code
  = help: declare 'exit' as an effect on this machine and call it with `perform`
```

Gust has no function declarations, so `helper(x)` named nothing the compiler
knew — and both backends emitted it verbatim, where it resolved against whatever
the generated file's module or package happened to have in scope.

**What to check:** any handler body containing a call that is not `perform`.
Calling a declared effect without `perform` is the common case and gets a
diagnostic saying exactly that. Anything else needs declaring as an effect and
implementing in the host.

This is the sandbox boundary. The security guide has always said a `.gu` is an
exhaustive list of how a component touches the outside world and that there is
no hidden call; as of 1.0 that is enforced rather than asserted.

## 5. Unknown generic type constructors are rejected

`Vec`, `Option`, and `Result` are the only generic constructors any backend
lowers. `HashMap<String, i64>` in a field is now an error rather than three
different downstream failures:

| Backend | Before |
|---|---|
| Go | emitted `HashMap[string, int64]` — no such type, does not compile |
| Rust | emitted `HashMap<String, i64>` with no `use std::collections::HashMap` |
| JSON Schema | emitted `{"description": "Unresolved generic type: HashMap"}` |

**What to do:** model a map as a `Vec` of a two-field `type`, or keep it in the
host and expose lookups as an effect. Map- and set-shaped names get that advice
in the diagnostic; near-misses like `List` and `Maybe` are pointed at `Vec` and
`Option`.

A real map type is a language feature — a schema representation and a lowering
per backend — and is a candidate for 1.x rather than something to infer from a
name that looks plausible.

## 6. Go builds refuse a `Result` error type Go cannot carry

An effect declared `-> Result<T, E>` lowers to Go's `(T, error)` idiom, so a
non-`String` `E` is erased: the `Err` binding holds a Go `error` where `E` was
expected, and the generated package does not compile.

This has warned since 0.4.0. As of 1.0 the *severity depends on what you asked
for*:

| Command | Behaviour |
|---|---|
| `gust check` | warns, passes — unchanged |
| `gust build --target rust` | succeeds — unchanged, `E` survives in Rust |
| `gust build --target go` | **fails, writes nothing** |
| `gust generate` (go target), `gust-build` `Target::Go`, MCP `gust_build` | **fails** |

Blocking it in `gust check` would penalise Rust-only users for a Go limitation.
Letting it reach a Go build emits a package that does not compile. Making it
conditional is the only answer that is right for both.

**What to do:** declare the effect as `-> Result<_, String>`, which round-trips
through `err.Error()`; or stop reading the `Err` binding, since an ignored
payload costs nothing; or build only the Rust target.

## 7. `gust-build` validates, and regenerates on content

Two changes to the `build.rs` helper.

**It validates.** It used to parse and emit without validating, so a `.gu` that
`gust check` rejects still produced output from `cargo build` — surfacing as a
host-compiler error against generated source you are told never to edit. It now
fails the build on validation errors and forwards warnings as `cargo:warning=`.
**A `.gu` that built before may now fail**, and an invalid source sitting in a
repository appears as a new failure with nothing having changed. Run `gust check`
to see the same diagnostics.

**It regenerates on content, not mtime.** Output was gated on the source being
*newer* than its `.g.rs`. After a fresh clone every file carries the checkout
timestamp, so a stale committed output was never rewritten. It now compares the
generated bytes. Identical output is still left untouched, so Cargo does not
rebuild every dependent crate on every build.

## 8. The `wasm` and `nostd` backends are removed

`gust build --target wasm` and `--target nostd` now exit non-zero.

Both emitted output that **compiled without implementing the source machine** —
the one thing a stability promise must not be extended over. `wasm` was the
worse of the two: state payload fields dropped (`state New(id: String)` became
the bare discriminant `New = 0`), every handler body dropped, no effect ever
invoked, and a multi-target transition always took its first target. The
`GustWasmEffectAdapter` trait it declared was referenced by nothing it emitted.
`nostd` kept state fields but emitted no handler bodies and no effects trait.

Neither was caught by CI, because the backend matrix compiles each fixture's
output with its real toolchain — which proves output is well-formed, not that it
does what the source says.

### Replacing `wasm`

Compile the **Rust** backend's output to `wasm32`:

```bash
gust build order.gu --target rust
```

Add the generated file to a `cdylib` crate, implement the generated
`{Machine}Effects` trait against your JavaScript bindings, and build for
`wasm32-unknown-unknown`.

This is strictly more capable than the removed backend. You keep handler bodies,
effects, and real state payloads — none of which survived before. Generic
machines work too, because nothing forces the machine itself through
`#[wasm_bindgen]`; only your hand-written wrapper crosses that boundary and you
choose its shape. The old backend could never support generics at all, since
`#[wasm_bindgen]` rejects type parameters outright.

### Replacing `nostd`

Use the Rust backend and adapt it in the host. Generated Rust depends on `serde`
and `thiserror`; where those are unavailable, the state enum and transition
methods are small enough to wrap by hand — and you pick your own capacities
rather than inheriting the emitter's hard-coded 64-byte strings and 16-element
vectors.

### Cleaning up

Leftover `.g.wasm.rs` and `.g.nostd.rs` files cannot be regenerated. Delete
them. `gust doctor` deliberately no longer lists them as freshness candidates,
because reporting one as "stale, regenerate" would send you after a flag that no
longer exists.

### Library consumers

`WasmCodegen`, `NoStdCodegen`, `Target::Wasm`, and `Target::NoStd` are gone from
`gust-lang` and `gust-build`.

## 9. `--target ffi` requires `--unstable-ffi`

```bash
gust build gate.gu --target ffi --unstable-ffi
```

The `.g.h` header is generated from the same AST as the Rust half, but **no CI
job compiles it** — verifying it would need a C toolchain in the pipeline.
Rather than freeze an unverified artefact into the 1.0 promise, this backend is
explicitly excluded from it and its output shape may change within 1.x.

The flag is the acknowledgement that an upgrade can break your header. Nothing
about the generated output changed in 1.0; only the opt-in is new.

`ffi` is still not a `gust.toml` manifest target, so it continues to need its
own `gust build` invocation.

---

## Next steps

- [Stability](stability.md) — what the 1.0 promise covers and what it does not.
- [Custom Targets](../advanced/custom_targets.md) — the surviving specialist
  backend, and what it takes to add one of your own.
- [Changelog](changelog.md) — the same changes organised by release mechanics.
