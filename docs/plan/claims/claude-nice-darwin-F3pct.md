# [WIP] wasm: implement scanTokens to return real tokens

- **Date**: 2026-05-09
- **Branch**: `claude/nice-darwin-F3pct`
- **PR**: TBD
- **Status**: claim
- **Workstream**: WASM API completeness (issue #4746)

## Intent

`scanTokens` in `crates/tsz-wasm/src/wasm_api/utilities.rs` currently
returns a hardcoded empty JSON array (`"[]"`) regardless of input. This
PR replaces the placeholder with a real implementation that drives the
existing `tsz_scanner::ScannerState` and emits the JSON token list
documented by the API: `[{ kind, text, start, end }, ...]`.

## Files Touched

- `crates/tsz-wasm/src/wasm_api/utilities.rs` (real `scan_tokens` body
  + unit tests)

## Verification

- `cargo nextest run -p tsz-wasm`
- `cargo build -p tsz-wasm`

Closes #4746.
