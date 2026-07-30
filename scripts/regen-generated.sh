#!/usr/bin/env bash
#
# Regenerate every Gust output file that is committed to this repository.
#
# Why this exists: the example projects call `gust_build::compile_gust_files()`
# from build.rs, but that helper is mtime-gated — it only rewrites an output
# when the `.gu` source is *newer* than the generated file. After a fresh clone
# every file shares one checkout timestamp, so a committed file that predates a
# codegen change stays stale indefinitely and only reappears as a surprise diff
# when someone happens to touch the `.gu`. Content, not mtime, is the thing to
# check, so this script is the single definition of how each committed artifact
# is produced. CI runs it and fails if the working tree moves.
#
# Run it after any codegen change and commit the result:
#
#     scripts/regen-generated.sh
#
# Set GUST to an already-built binary to skip the cargo rebuild:
#
#     GUST=target/debug/gust scripts/regen-generated.sh
#
set -euo pipefail

cd "$(dirname "$0")/.."

if [[ -n "${GUST:-}" ]]; then
    gust() { "$GUST" "$@"; }
else
    gust() { cargo run -q -p gust-cli -- "$@"; }
fi

# Examples whose build.rs writes Rust output next to each `.gu` source. Keep
# this list in step with the `.g.rs` files tracked by git.
rust_in_place=(
    examples/event_processor/src/processor.gu
    examples/microservice/src/order.gu
    examples/microservice/src/payment.gu
    examples/microservice/src/supervisor.gu
    examples/workflow_engine/src/engine_failure.gu
    examples/workflow_engine/src/workflow.gu
)

for src in "${rust_in_place[@]}"; do
    gust build "$src" --output "$(dirname "$src")"
done

# The standalone showcase machine at the examples root ships both backends.
# Its Go output is `package main` because the file doubles as a runnable
# program in the README walkthrough.
gust build examples/order_processor.gu --output examples
gust build examples/order_processor.gu --target go --package main --output examples

# shared_contracts drives all three of its targets (rust, go, schema) from its
# gust.toml, so the manifest — not this script — owns their output paths.
gust generate --config examples/shared_contracts/gust.toml
