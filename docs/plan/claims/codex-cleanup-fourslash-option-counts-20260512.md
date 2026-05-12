# chore(lsp): simplify fourslash optional counts

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-fourslash-option-counts-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: DRY cleanup

## Intent

Replace repeated `Option<Vec<_>>` count spelling in fourslash assertion helpers
with `map_or(0, Vec::len)`. This keeps the assertion behavior unchanged while
making the helper code easier to scan.

## Files Touched

- `crates/tsz-lsp/src/fourslash.rs` (~7 mechanical call-site cleanups)

## Verification

- `cargo nextest run -p tsz-lsp --lib --no-fail-fast`
