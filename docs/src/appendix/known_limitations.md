# Known Limitations

This page tracks current, intentional limitations in `v0.1.0`.

## Nested Cargo Workspaces and `gust init`

`gust init <name>` now detects parent Cargo workspaces and automatically adds `[workspace]` to the generated project's `Cargo.toml` when needed.

### For Older Generated Projects

If you scaffolded a project before this behavior existed and it fails due to workspace nesting:

- Add an empty `[workspace]` table to the generated project's `Cargo.toml`.
- Or move the generated project outside the parent workspace.

## Inter-machine Transport Scope

Inter-machine communication currently uses local in-process channels. Cross-process/network transport is not part of `v0.1.0`.

## LSP Symbol Editing

Rename and find-references are intentionally disabled in `v0.1.0`.

The current symbol model is not yet scope-aware enough to guarantee safe edits across identifiers, comments, and string literals. Those features will come back once the LSP can resolve symbols structurally instead of relying on textual matches.

## Documentation Maturity

Core workflows are tested and stable, but some guide/cookbook pages are still evolving and may expand in future releases.
