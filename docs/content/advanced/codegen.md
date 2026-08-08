---
title: "Code Generation"
description: "How a .gu file becomes Rust or Go — the compiler pipeline, the six emitters behind it, and the real shape of the code they produce."
type: reference
---

# Code Generation

Gust is a source-to-source compiler. There is no Gust runtime executing your machine, no interpreter, and no reflection: a `.gu` file becomes a Rust file or a Go file, and from that point on it is ordinary Rust or ordinary Go. Understanding what comes out the other end is most of what you need to integrate it well.

## The pipeline

```text
source.gu
  → parser  (pest PEG grammar)
  → AST     (nodes carrying spans)
  → validator
  → backend → .g.rs / .g.go / …
```

Each stage has one job.

**The parser** is a PEG grammar (`grammar.pest`) plus a hand-written pass that turns pest's parse pairs into typed AST nodes. It rejects anything the grammar does not describe. Because it is a PEG, the first alternative that matches wins — which is why some things that look writable are not. Constructing a payload-carrying enum variant is the classic case: the qualified-path rule is tried before the function-call rule, so `Failure::Timeout(500)` matches `Failure::Timeout` and then fails at the `(`.

**The AST** carries source spans, and this is what makes the diagnostics readable. Spans currently reach top-level nodes — declarations, `goto`, `perform`, `send`, `spawn` — but not individual expressions, so a diagnostic about a subexpression points at the enclosing statement rather than the expression itself.

**The validator** is where meaning is checked: unknown state names, `goto` targets that the transition does not declare, effect arity, unused bindings, action placement, channel annotations. It produces errors, which are fatal, and warnings, which are not. Errors say what is wrong; a `note` says why; a `help` says what to do, with did-you-mean suggestions matched fuzzily against your declared names.

**The backend** walks the validated AST and emits text. It does no checking of its own.

### Validation runs before anything is emitted

Every emitting path validates first. `build`, `watch`, and `generate` all run
the validator before rendering, and a semantic error stops them:

```bash
gust check order.gu     # parse + validate, no output
gust build order.gu     # parse + validate + generate
```

A machine whose handler does `goto Nowhere()` fails both, and `build` writes
nothing — it does not even create the output directory. Warnings print and do
not block, which matters because some of them are backend-specific: an unused
binding is a warning in Rust and a hard error in Go.

::: callout note "Changed after 0.3.0"
In 0.3.0 only `check`, `schema`, and `doctor` validated. `gust build` parsed
and generated, so a `.gu` that `gust check` rejected still produced a `.g.rs`
and exited 0 — the mistake surfaced later as a `rustc` or `go build` error in a
file you were told never to edit. On that release, chain the two yourself.
:::

## Four emitters, one AST

Three targets are reachable from `gust build`, plus a JSON Schema emitter that has its own subcommand.

| Target | Output | Emitter | Purpose |
|---|---|---|---|
| `rust` | `.g.rs` | `RustCodegen` | The default. Full handler bodies, effects trait, async. |
| `go` | `.g.go` | `GoCodegen` | Full handler bodies, effects interface. Needs `--package`. |
| `ffi` | `.g.ffi.rs` + `.g.h` | `CffiCodegen` | Handle-based C ABI, plus a header. State transitions only. Requires `--unstable-ffi`. |
| — | `.schema.json` | `SchemaCodegen` | JSON Schema for states and types. `gust schema`. |

They share the AST and a small pool of helpers — name-case conversion, known-type collection, the `ctx` rule below, Mermaid diagram generation — and nothing else. There is no common backend trait, and the emitters are not symmetric in what they support. Rust and Go are the two production targets; `ffi` is covered in [Custom Targets](custom_targets.md).

The `wasm` and `nostd` emitters were removed in 1.0. Both compiled while implementing none of the source machine's behaviour, which is not something a stability promise can be extended over. To target WebAssembly, compile the Rust backend's output to `wasm32`.

If you drive the emitters directly as a library rather than through the CLI, expect their signatures to be inconsistent with each other. `RustCodegen::generate` and `GoCodegen::generate` consume `self`; `CffiCodegen` borrows it. `CffiCodegen::generate` returns a `(source, header)` tuple rather than a string. `SchemaCodegen::generate` is an associated function with no receiver at all. `GoCodegen::generate` takes a package name that nothing else needs.

### Three surfaces, three sets of targets

Where you invoke Gust from decides which targets you can reach.

| Surface | Targets |
|---|---|
| `gust build --target` | `rust`, `go`, `ffi` (with `--unstable-ffi`) |
| `gust.toml` manifest | `rust`, `go`, `schema` |
| `gust-build` in `build.rs` | `rust`, `go`, `ffi` |

The manifest gap is the one that catches people. There is no `[targets.ffi]` — a manifest that declares one is rejected. If you want C FFI output in a project that otherwise uses [contract packages](contract_packages.md), those files need a separate `gust build` invocation. Conversely, JSON Schema is a manifest target and a subcommand, but not a `gust build --target`.

## What generated Rust looks like

Take a machine with one transition, one state field, and one effect:

```gust
machine Gate {
    state Closed(id: String)
    state Open(id: String, opened_by: String)

    transition open: Closed -> Open

    effect authorise(id: String) -> String

    on open(ctx) {
        let who = perform authorise(ctx.id);
        goto Open(ctx.id, who);
    }
}
```

`gust build gate.gu` produces this:

```rust "gate.g.rs"
// Generated by Gust compiler — do not edit manually
use serde::{Serialize, Deserialize};
use gust_runtime::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GateState {
    Closed {
        id: String,
    },
    Open {
        id: String,
        opened_by: String,
    },
}

pub trait GateEffects {
    /// gust:effect -- replay-safe / idempotent
    fn authorise(&self, id: &str) -> String;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gate {
    pub state: GateState,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum GateError {
    #[error("invalid transition '{transition}' from state '{from}'")]
    InvalidTransition { transition: String, from: String },
    #[error("transition failed: {reason}")]
    Failed { reason: String },
}

impl Gate {
    pub fn new(id: String) -> Self {
        Self { state: GateState::Closed { id } }
    }

    pub fn state(&self) -> &GateState {
        &self.state
    }

    pub fn open(&mut self, effects: &impl GateEffects) -> Result<(), GateError> {
        match &self.state {
            GateState::Closed { id } => {
                let id = id.clone();
                let who = effects.authorise(&id);
                self.state = GateState::Open { id, opened_by: who };
                Ok(())
            }
            _ => Err(GateError::InvalidTransition {
                transition: "open".to_string(),
                from: format!("{:?}", self.state),
            }),
        }
    }
}
```

Several decisions are visible there.

**States are one enum with struct variants.** Field names come straight from the `.gu`, so `GateState::Open { id, opened_by }` is what you match on. Illegal states are unrepresentable, which is the whole point.

**Transitions are methods that return `Result`.** Calling `open()` from the wrong state returns `Err(GateError::InvalidTransition)` rather than panicking. The `_ =>` arm is that guard.

**Effects become a trait, and the machine takes it as an argument.** `GateEffects` has one method per declared effect, taking `&self` and borrowing its arguments. The transition method takes `effects: &impl GateEffects`. A transition whose handler performs nothing takes no `effects` argument at all — so the signature tells you whether calling it can reach the outside world.

**Source-state fields are destructured in the match arm.** That is how `ctx.id` becomes a plain `id`, and why bare field references work in the Rust backend.

**The `gust:effect` doc comment is deliberate.** `effect` and `action` generate identical code; the marker is the only surviving trace of which keyword you used, and replay-aware runtimes read it.

### Generated Rust is not standalone

The prelude gives it away. The output derives `Serialize`/`Deserialize`, derives `thiserror::Error`, and imports `gust_runtime::prelude::*`. All three dependencies must be present in the consuming crate:

```toml "Cargo.toml"
[dependencies]
gust-runtime = "0.3"
serde = { version = "1", features = ["derive"] }
thiserror = "2"
```

Codegen writes a file and stops. Wiring it into your module tree is on you — and prefer `include!` over `#[path] mod`, because `cargo fmt` follows `mod` declarations and will reformat generated files behind your back, which then breaks `gust generate --check`.

## What generated Go looks like

Same source, `gust build gate.gu --target go --package gate`:

```go "gate.g.go"
// Code generated by Gust compiler — DO NOT EDIT.

package gate

import (
	"encoding/json"
	"fmt"
)

var _ = json.Marshal
var _ = fmt.Errorf

type GateState int

const (
	GateStateClosed = iota
	GateStateOpen
)

// func (s GateState) String() string — elided

type GateClosedData struct {
	Id string `json:"id"`
}

type GateOpenData struct {
	Id string `json:"id"`
	OpenedBy string `json:"opened_by"`
}

type GateEffects interface {
	// gust:effect -- replay-safe / idempotent
	Authorise(id string) string
}

type Gate struct {
	State GateState `json:"state"`
	ClosedData *GateClosedData `json:"closed_data,omitempty"`
	OpenData *GateOpenData `json:"open_data,omitempty"`
}

func (m *Gate) clearStateData() {
	m.ClosedData = nil
	m.OpenData = nil
}

func NewGate(id string) *Gate {
	return &Gate{
		State: GateStateClosed,
		ClosedData: &GateClosedData{
			Id: id,
		},
	}
}

// type GateError + func (e *GateError) Error() string — elided

func (m *Gate) Open(effects GateEffects) error {
	if m.State != GateStateClosed {
		return &GateError{Transition: "open", From: m.State.String()}
	}

	who := effects.Authorise(m.ClosedData.Id)
	var __goto_open_id string = m.ClosedData.Id
	m.State = GateStateOpen
	m.clearStateData()
	m.OpenData = &GateOpenData{
		Id: __goto_open_id,
		OpenedBy: who,
	}

	return nil
}

// func (m *Gate) ToJSON / func GateFromJSON — elided
```

The model is the same and the encoding is not, because Go has no sum types.

**States become an integer constant plus one struct per state**, held as nullable pointers on the machine with a `clearStateData` helper that nils them all before the new one is set. Only the pointer matching the current `State` is non-nil. There is no compiler guarantee of that — it is an invariant the generated code maintains, which is the price of the encoding.

**Effects become an interface**, and transition methods return a plain `error`.

**The temporary `__goto_open_id` is not noise.** `clearStateData()` releases the old state's data, so any field being carried forward has to be read into a local first. Values are captured before the switch, then the new state is built.

**The output is self-contained.** Its only imports are `encoding/json` and `fmt`, both from the standard library, and both are unconditionally referenced by the `var _ =` lines so the file compiles whether or not a given machine happens to use them. Every file also gets `ToJSON` and `FromJSON` helpers, elided above. There is no Go equivalent of `gust-runtime`, and none is needed.

## Where the two backends diverge

Both consume the same validated AST, so the machine model is identical. The lowerings are not, and the differences are worth knowing before you commit to both targets.

**`Result` becomes Go's two-value idiom.** An effect declared `-> Result<T, E>` is a single fallible value in Rust and a `(T, error)` pair in Go. A `match` over it lowers accordingly:

```gust
machine Charge {
    state Pending(amount: i64)
    state Paid(receipt: String)
    state Failed(reason: String)

    transition settle: Pending -> Paid | Failed

    effect charge(amount: i64) -> Result<String, String>

    on settle(ctx) {
        let outcome = perform charge(ctx.amount);
        match outcome {
            Ok(receipt) => { goto Paid(receipt); }
            Err(reason) => { goto Failed(reason); }
        }
    }
}
```

Go gets `Charge(amount int64) (string, error)`, and the `Ok`/`Err` match becomes a nil check on the error. The consequence is that **`E` does not survive**: Go signals failure with one `error` type. `String` round-trips through `err.Error()`; any other error type arrives in the `Err` binding as a Go `error` instead. The validator warns when it sees an `E` that cannot make the trip.

**Unused locals are a Go error and a Rust warning.** `declared and not used` is fatal to `go build`; Rust merely warns, though `-D warnings` promotes it. Both backends lower an unread binding to a discard so the output compiles either way, and the validator warns against the `.gu` — one message, at the source, rather than a surprise from one toolchain.

**Rust has a real `Debug` for the current state; Go has a `String()`.** The invalid-transition error formats the state with `{:?}` in Rust and `State.String()` in Go.

::: callout tip "Compile every target you ship"
`gust check` validates Gust, not the code Gust emits. The two backends have historically drifted, and the only reliable defence is running `cargo clippy -D warnings` and `go build` over the real output. Gust's own test suite does exactly this — its earlier string-matching tests happily passed on output that no compiler had ever accepted.
:::

## The `ctx` rule is a codegen decision

Look again at the `Gate` handler from the top of this page:

```text
on open(ctx) {
    let who = perform authorise(ctx.id);
    goto Open(ctx.id, who);
}
```

Both backends identify the context parameter with the same rule: **it is the handler parameter with no type annotation**, and it must be named `ctx`. That parameter is dropped from the generated signature, and `ctx.field` resolves against the source state. Every parameter that *does* carry a type becomes a real argument.

::: callout info "This changed in 1.0"
The rule used to be *"the first handler parameter whose type is not a declared type"*, so the idiom was written `on open(ctx: OpenCtx)` with `OpenCtx` declared nowhere.

That made "the compiler does not know this name" load-bearing syntax, and it had three consequences. Misspelling a type silently turned that parameter into the accessor and dropped it — `on start(cfg: Confgi)` compiled to a method with no `cfg`, and `gust check` could not object, because an undeclared type name in that position was the intended idiom. A machine's own generic parameters had to be special-cased, or `on put(value: T)` on `machine Box<T>` lost its argument. And **every type name the compiler might learn later would silently change handler signatures that already compiled** — which made growing the type system a breaking change against source nobody had edited.

An absent annotation cannot drift that way. `BUILTIN_TYPES`, `collect_known_types`, and `machine_known_types` were deleted along with the rule.
:::

A generic machine's type parameter is now simply a parameter type, needing no special handling:

```gust
machine Box<T> {
    state Empty
    state Full(item: T)

    transition put: Empty -> Full

    on put(value: T) {
        goto Full(value);
    }
}
```

That now generates `pub fn put(&mut self, value: T)`, and Go instantiates the generic state-data struct properly rather than referring to it bare.

## Reading the output before you trust it

Two commands help when the generated code is not what you expected.

```bash
gust parse order.gu      # the AST, as the backends see it
gust diagram order.gu    # a Mermaid state diagram
```

`gust parse` prints the AST after parsing but before validation. It is the fastest way to confirm that what you wrote is what the compiler read — the context-parameter rule in particular is much easier to see in the AST than in the source. `gust diagram` renders the state graph, which catches a transition wired to the wrong target long before any code is emitted.

## Next steps

- [Contract Packages](contract_packages.md) — generating several targets reproducibly from one manifest, and checking the result in CI.
- [Custom Targets](custom_targets.md) — C FFI, the targets removed in 1.0 and what replaced them, and what it takes to add a target of your own.
