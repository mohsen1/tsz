# wasm: implement scanTokens API

- **Date**: 2026-05-09
- **Branch**: `claude/nice-darwin-kwWNx`
- **PR**: TBD
- **Status**: claim
- **Workstream**: WASM API parity (issue #4746)

## Intent

`scanTokens` in the WASM API currently returns a hardcoded `[]` regardless of
input. This claim implements the function by driving `tsz_scanner::ScannerState`
in `skip_trivia` mode and serializing the resulting `(kind, text, start, end)`
tokens as JSON, matching the existing `TokenInfo` shape that was already
declared but never populated.

## Files Touched

- `crates/tsz-wasm/src/wasm_api/utilities.rs` (~25 LOC change)
- `crates/tsz-wasm/src/wasm_tests.rs` (~50 LOC test)

## Verification

- `cargo test -p tsz-wasm` (covers the new `scan_tokens` tests)
