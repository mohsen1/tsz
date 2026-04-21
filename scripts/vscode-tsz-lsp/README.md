## TSZ VS Code LSP Dev Client

This is a minimal local VS Code extension that launches `tsz-lsp` over stdio.

### One-time setup

```bash
cargo build -p tsz-cli --bin tsz-lsp
cd scripts/vscode-tsz-lsp
npm install
npm run compile
```

### Run in VS Code

1. Open the repo in VS Code.
2. Run the launch configuration `Run TSZ VS Code Client`.
3. In the Extension Development Host, open a `.ts` or `.js` file.
4. Use `TSZ: Restart Language Server` after rebuilding the Rust binary.

By default the extension looks for `target/debug/tsz-lsp` in the first workspace folder. You can override this with the `tsz.lsp.path` setting.
By default in this repo that resolves to `.target/debug/tsz-lsp`, because Cargo is configured to build into `.target`.