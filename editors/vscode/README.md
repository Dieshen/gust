# Gust Language Support

VS Code extension for the Gust language:

- Syntax highlighting for `.gu`
- Snippets for common machine patterns
- File nesting for generated `.g.rs` and `.g.go`
- LSP client integration with `gust-lsp`

## Development

```bash
cd editors/vscode
npm install
npm run compile
```

Build VSIX:

```bash
npm run package
```
