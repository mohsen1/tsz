# wasm: scanTokens emits real token list

- **Date**: 2026-05-09
- **Branch**: `claude/nice-darwin-P8Z3n`
- **PR**: TBD
- **Status**: claim
- **Workstream**: WASM API parity

## Intent

Fix issue #4746. The WASM `scanTokens` API parses input but discards the
result and always returns `"[]"`, regardless of the source text. This change
drives the existing `tsz-scanner::ScannerState` directly and emits a real
JSON array of `TokenInfo { kind, text, start, end }` entries — including
trivia — matching the documented contract.

## Files Touched

- `crates/tsz-wasm/src/wasm_api/utilities.rs` (~30 LOC change)
- `crates/tsz-wasm/src/wasm_tests.rs` (~40 LOC of new tests)

## Verification

- `cargo nextest run -p tsz-wasm`
