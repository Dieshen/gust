# Package Registry Design

## Goals
- Discoverable reusable Gust machines
- Versioned package publishing
- Deterministic builds and lockfiles

## Proposal
1. Registry namespace `gust://org/package@version`
2. Package metadata in `gust.toml`
3. Sign package tarballs and verify checksum
4. Resolve transitive dependencies with semver

## MVP
- Read-only index
- CLI install command
- Local cache in `$HOME/.gust/registry`
