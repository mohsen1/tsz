# wasm: implement scanTokens API to return real tokens

- **Date**: 2026-05-09
- **Branch**: `claude/nice-darwin-ZVHIm`
- **PR**: TBD
- **Status**: claim
- **Workstream**: WASM API parity (closes #4746)

## Intent

`crates/tsz-wasm/src/wasm_api/utilities.rs` currently returns a hardcoded
`"[]"` for the `scanTokens` JS-bound entry point regardless of input. Wire
the function up to `tsz_scanner::ScannerState` so it produces real
`{ kind, text, start, end }` token JSON, matching the documented
TokenInfo shape, and add a unit test that exercises a few token kinds.

## Files Touched

- `crates/tsz-wasm/src/wasm_api/utilities.rs` (replace placeholder body
  with real scanner driver)
- `crates/tsz-wasm/src/wasm_tests.rs` (add `scan_tokens` parity test)

## Verification

- `cargo nextest run -p tsz-wasm`
