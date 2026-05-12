# chore(lsp-tests): share selection range parse helper

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-lsp-selection-parse-helper-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: DRY cleanup

## Intent

The selection-range LSP tests still carry a local parser setup helper with the same parse-source shape used across the LSP test cleanup stream.
This PR keeps the slice focused by moving that setup behind a small helper in the selection-range module and updating the mounted tests to use it.

## Files Touched

- `crates/tsz-lsp/src/editor_ranges/selection_range.rs`
- `crates/tsz-lsp/tests/selection_range_tests.rs`

## Verification

- Pending
