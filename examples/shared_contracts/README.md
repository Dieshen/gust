# Shared Contracts Example

This example keeps one Gust contract package beside separate Rust and Go
consumers:

```text
shared_contracts/
  gust.toml
  gu-contracts/
  rs-project/
  go-project/
```

Generate every configured target from the directory that contains `gust.toml`:

```sh
gust generate
```

Or generate one target:

```sh
gust generate --target go
gust generate --target rust
gust generate --target schema
```

CI can verify committed generated files without rewriting them:

```sh
gust generate --check
```

After generation, the host projects can validate their side normally:

```sh
cd go-project && go test ./...
cd rs-project && cargo check
```
