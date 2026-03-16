# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes       |

## Reporting a Vulnerability

**Do not open a public issue for security vulnerabilities.**

Use GitHub's private vulnerability reporting:

1. Go to the [Security tab](https://github.com/Dieshen/gust/security) of this repository
2. Click **"Report a vulnerability"**
3. Provide a description, reproduction steps, and affected components

### Scope

The following components are in scope:

- Gust compiler (`gust-lang`) — parser, validator, code generators
- Generated code patterns (`.g.rs`, `.g.go`) — unsafe output, injection risks
- CLI (`gust-cli`) — file handling, path traversal
- LSP server (`gust-lsp`) — input handling
- MCP server (`gust-mcp`) — JSON-RPC input handling
- Build integration (`gust-build`) — build script safety

### What to Expect

- **Acknowledgment** within 3 business days
- **Assessment and plan** within 10 business days
- **Fix or mitigation** timeline communicated after assessment

### Out of Scope

- Vulnerabilities in dependencies (report upstream; we will update when patches are available)
- Denial of service via extremely large `.gu` input files (known resource limitation)
