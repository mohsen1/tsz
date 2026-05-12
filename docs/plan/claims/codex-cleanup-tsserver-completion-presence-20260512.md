# chore(tsserver): simplify completion auto-import presence guards

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-tsserver-completion-presence-20260512`
- **PR**: #5695
- **Status**: ready
- **Workstream**: 8.4 (DRY emitter helpers)

## Intent

This PR cleans up tsserver completion auto-import guard clauses that combine
action metadata checks with optional completion metadata checks. The change
keeps behavior identical while making the required presence checks read
directly in the completion and snippet flows.

## Files Touched

- `crates/tsz-cli/src/bin/tsz_server/handlers_completions.rs`
- `crates/tsz-cli/src/bin/tsz_server/handlers_completions_snippets.rs`

## Verification

- `cargo fmt --check` passed.
- `cargo nextest run -p tsz-cli completion` passed: 90 passed, 1515 skipped.
- `cargo clippy -p tsz-cli --all-targets -- -D warnings` passed.
- `cargo nextest run -p tsz-cli` ran the full local `tsz-cli` suite: 1506 passed, 85 failed, 14 skipped. Failures match the existing local baseline families around driver compile snapshots, unsupported `es2025.iterator`, showConfig path rendering, and help-all parity.
