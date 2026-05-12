# chore(tsserver): simplify completion response flags

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-tsserver-completion-flags-20260512`
- **PR**: #5850
- **Status**: ready
- **Workstream**: DRY cleanup

## Intent

Simplify repeated optional completion-result boolean extraction in the tsserver
completion response builder. This keeps the JSON response behavior unchanged
while making the default-false flag handling easier to scan.

## Files Touched

- `crates/tsz-cli/src/bin/tsz_server/handlers_completions.rs` (~3 mechanical call-site cleanups)

## Verification

- `cargo nextest run -p tsz-cli --bin tsz-server --no-fail-fast` (369 tests pass, 3 skipped)
