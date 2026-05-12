# chore(tsserver): simplify completion response flags

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-tsserver-completion-flags-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: DRY cleanup

## Intent

Simplify repeated optional completion-result boolean extraction in the tsserver
completion response builder. This keeps the JSON response behavior unchanged
while making the default-false flag handling easier to scan.

## Files Touched

- `crates/tsz-cli/src/bin/tsz_server/handlers_completions.rs` (~3 mechanical call-site cleanups)

## Verification

- `cargo nextest run -p tsz-cli --bin tsz_server --no-fail-fast`
