# Compatibility corpus and golden output

Two properties over one frozen set of sources, asserted by
[`../compat_corpus.rs`](../compat_corpus.rs).

## The rule

**Files in a release directory are never edited after that release ships.**

That is the whole point. If a corpus file is rewritten to satisfy a new
validator rule, the test stops meaning *"source that compiled at 1.0 still
compiles"* and starts meaning *"source we were willing to change still
compiles"* — which is not a promise at all.

If a corpus source stops validating, the compiler is what changed. Either fix
it, or make the break deliberately: record it in `CHANGELOG.md`, write it into
the upgrade guide, and open the next release's directory.

## Layout

```
compat/
  v1.0/
    retry.gu        # frozen source
    retry.g.rs      # golden — generated Rust
    retry.g.go      # golden — generated Go
```

The directory name is the release that introduced those sources. New sources go
in the newest directory; existing directories only ever gain goldens for new
backends.

## What each test asserts

**`every_corpus_source_still_compiles`** — every `.gu` parses and validates with
no errors. This is the compatibility promise.

**`generated_output_matches_the_goldens`** — generated Rust and Go match what is
recorded. Codegen is deterministic, so this is cheap and exact.

A golden diff is **not** a failure to be silenced. It is a decision: either the
change was unintended (that is the bug), or it was intended and needs a
`CHANGELOG.md` entry before the goldens are re-recorded.

## Updating goldens

```bash
UPDATE_GOLDENS=1 cargo test -p gust-lang --test compat_corpus
```

Then read `git diff`. Re-recording without reading the diff defeats the test.

## Adding sources

Drop a `.gu` into the newest release directory and re-record. **Real projects are
wanted here.** The corpus is only as good as the shapes in it, and everything
else in the test suite is deliberately small.

Two things this corpus is not:

- It does not compile the generated output. `codegen_backends.rs` does that, on
  a curated fixture set, with each backend's real toolchain.
- It does not run the generated machines. `go_behaviour.rs` does that, for the
  defects where compiling proved nothing.

## `regressions.gu`

Purpose-built, unlike the rest of the corpus, which is real code lifted from the
examples, the standard library, and the project template.

It exists because of a measurement. Reverting the fix for #121 — `goto` must end
the handler — moved exactly **one** golden out of nineteen, because real-world
handlers overwhelmingly end in a single tail `goto`, and the shape that broke is
an early `goto` inside a bare `if`. Real code is the right corpus for
compatibility and a thin one for regression detection.

`regressions.gu` gathers the shapes that have actually broken: branching `goto`
with and without an `else`, `supervises`/`spawn` with a constructor argument,
`Result` matching, a generic machine, and bare source-state field reads. With it
in place the same revert moves two goldens.

Do not tidy it. The awkward shapes are the point.
