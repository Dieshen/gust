---
title: "Contract Packages"
description: "Share one set of .gu contracts across Rust and Go projects with a gust.toml manifest, and keep the generated files honest in CI."
type: reference
---

# Contract Packages

A contract package is a directory of shared `.gu` sources plus a `gust.toml`
manifest declaring one or more generated targets. It is the intended shape when
a Rust project and a Go project have to agree on the same machines:

```text
repo/
  gu-contracts/
    order.gu
  rs-project/
  go-project/
  gust.toml
```

The alternative — one `gust build` invocation per file per target, in a shell
script somebody has to remember to update — drifts. A manifest makes the set of
sources and the set of outputs a single declared thing, which is what makes
`--check` in CI possible.

Run generation from the directory that contains `gust.toml`:

```sh
gust generate
```

If `--config` is omitted, `gust generate` looks for `./gust.toml` in the current
directory. Paths inside the manifest are resolved relative to the manifest file,
not the shell's working directory.

::: callout warning "Generation does not validate"
`gust generate` parses and emits. It does not run the validator, so a machine
with a semantic error still produces output — output that fails to compile in
the host language. Run `gust check` over your sources first, in the same CI job.
:::

## Manifest

```toml "gust.toml"
[package]
name = "shared-contracts"
version = "0.1.0"
id = "gust://local/shared-contracts@0.1.0"

[source]
root = "gu-contracts"
include = ["**/*.gu"]
exclude = ["**/*.draft.gu"]

[targets.rust]
output = "rs-project/src/generated"
module = "contracts"
tracing = false

[targets.go]
output = "go-project/internal/contracts"
package = "contracts"

[targets.schema]
output = "schemas"
```

`[package]` is metadata for package identity and future registry workflows. Local
generation currently uses `[source]` and `[targets.*]`.

`[source]` selects the shared Gust inputs. `include` defaults to `["**/*.gu"]`;
`exclude` is optional.

`[targets.rust]` emits one `*.g.rs` file per source file. The `module` value
records the intended host module name; the generated file is not wrapped in a
Rust `mod`, so the Rust project decides whether to use `mod`, `include!`, or a
build script. `tracing = true` adds `tracing` spans and events around every
transition and effect, guarded by `#[cfg(feature = "tracing")]` so the
dependency stays optional for consumers who do not enable it.

`[targets.go]` emits one `*.g.go` file per source file. `package` is required
and becomes the Go package declaration for every generated file in that target.

`[targets.schema]` emits one `*.schema.json` file per source file.

Generated filenames are based on the source stem: `order.gu` becomes
`order.g.rs`, `order.g.go`, and `order.schema.json`. A target fails if multiple
matched source files would write the same generated path.

### Only three targets are manifest targets

`rust`, `go`, and `schema` are the whole list. There is no `[targets.wasm]`,
`[targets.nostd]`, or `[targets.ffi]` — those three are reachable only from
`gust build --target` and from `gust-build` in a `build.rs`. If a contract
package also needs WebAssembly or C FFI output, that is a separate invocation
alongside `gust generate`. See [Custom Targets](custom_targets.md).

## Where a Manifest May Write

A `gust.toml` is untrusted input — cloning a repository and running
`gust generate` inside it is an ordinary thing to do, and the manifest came
with the repository. Target outputs are therefore confined: a path must resolve
beneath either the directory holding the manifest or the directory you ran
from.

```toml
[targets.rust]
output = "rs-project/src/generated"   # fine
```

```toml
[targets.rust]
output = "../../../../etc/cron.d"     # refused
output = "/usr/local/bin"             # refused
```

Containment is checked after normalising `.` and `..`, so an escape buried
mid-path (`gu-contracts/../../../elsewhere`) is refused too.

Two roots are permitted because both are chosen by you rather than by the
manifest. That keeps the layout where a manifest lives in a subdirectory and
emits into sibling projects working:

```sh
gust generate --config config/gust.toml   # output = "../rs-project/src/generated"
```

Pass `--allow-outside` to lift the restriction when an escaping path is
genuinely what you want:

```sh
gust generate --allow-outside
```

## Target Selection

Generate every configured target:

```sh
gust generate
```

Generate one target:

```sh
gust generate --target rust
gust generate --target go
gust generate --target schema
```

Verify generated files in CI without rewriting them:

```sh
gust generate --check
gust generate --target go --check
```

`--check` exits with an error when a generated file is missing or stale.

## Keeping committed output honest

If you commit generated files — the usual choice for a contract package, since
it lets consumers build without the Gust toolchain — then `--check` is what
stops them rotting. A reasonable CI step:

```sh
for f in gu-contracts/*.gu; do gust check "$f" || exit 1; done
gust generate --check
```

The loop is not an oversight: `gust check` takes exactly one file. The first
step catches semantic errors that generation would have emitted straight past.
The second catches a `.gu` edited without regenerating, and a compiler upgrade
that changed the emitted output.

Two things quietly break `--check`, and both are worth knowing before they
bite you:

- **`cargo fmt` reformatting generated Rust.** rustfmt follows `mod`
  declarations, so wiring a `.g.rs` in with `#[path] mod` puts it in rustfmt's
  path and the next `cargo fmt` rewrites it. `include!` is not followed, and is
  the safer wiring.
- **Timestamp-gated regeneration.** `gust-build` in a `build.rs` compares
  modification times. After a fresh clone every file shares one checkout
  timestamp, so a stale committed output is never rewritten and the drift only
  appears when someone happens to touch a `.gu`. `gust generate --check`
  compares content, not times, which is why it belongs in CI regardless of how
  you generate locally.

## Import Semantics

Gust `use` paths are target-specific today.

Rust generation emits non-`std` imports as Rust `use` statements, for example
`use crate::domain::OrderId;`. Gust's virtual `std::*` namespace is stripped so
it does not collide with Rust's standard library crate.

Go generation maps non-`std` import paths from Gust's `::` syntax into Go import
paths, for example `use github::com::acme::payments;` becomes
`"github.com/acme/payments"`. Gust's virtual `std::*` namespace is also stripped
for Go.

For shared Rust and Go contract packages, the stable MVP is to keep contract
types that must be shared across targets in the same matched `.gu` source set and
generate them into a single Go package. Cross-package type references are
allowed by the current code generators only when the host project provides the
matching Rust crate module or Go import path.

See `examples/shared_contracts` for a complete layout.

## Next steps

- [Code Generation](codegen.md) — what the `rust`, `go`, and `schema` emitters
  actually produce, and where the two production backends diverge.
- [Custom Targets](custom_targets.md) — the three targets a manifest cannot
  reach.
