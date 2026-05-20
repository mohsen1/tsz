# LSP Smoke Tests

`e2e-smoke.mjs` starts the real `tsz-lsp` stdio binary and speaks
Content-Length framed JSON-RPC. The smoke intentionally stays narrow: it checks
server lifecycle, `didOpen`, pull diagnostics, completion, hover, definition,
rename, shutdown, and exit.

Fourslash remains the broad language-service parity signal. Failures in a
single feature's TypeScript behavior usually belong in fourslash or focused
`tsz-lsp` crate tests. Failures in process startup, protocol framing, document
lifecycle, or request/response wiring belong in this smoke.

The current WASM gate is still the existing `wasm` / `wasm-web` CI pair. A
browser-facing LSP smoke should be added here once the LSP and WASM paths share
the compiler-service front door named by the roadmap.
