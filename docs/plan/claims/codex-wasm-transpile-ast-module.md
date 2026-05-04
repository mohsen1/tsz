# codex-wasm-transpile-ast-module

Status: ready
Branch: `codex/wasm-transpile-ast-module`
Started: 2026-05-04T14:24:57Z
PR: #2706

## Scope

- Harden `crates/tsz-wasm/src/wasm_api/emit.rs` transpile helpers.
- Replace source-string module syntax detection with parsed AST/source-file checks.
- Add `fileName` support for `transpileModule`, defaulting through one named constant.
- Return a structured emit diagnostic for invalid transpile option JSON.
- Extract the repeated parse/lower/print path into one helper.

## Verification

- `cargo fmt --check`
- `cargo test -p tsz-wasm transpile -- --nocapture`
