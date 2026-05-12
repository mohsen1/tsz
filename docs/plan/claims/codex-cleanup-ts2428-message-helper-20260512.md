# chore(checker-tests): reuse TS2428 diagnostic helper

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-ts2428-message-helper-20260512`
- **Base**: `origin/main`
- **Issue**: n/a
- **PR**: [#6056](https://github.com/mohsen1/tsz/pull/6056)
- **Status**: ready
- **Labels**: `DRY`, `checker-tests`

## Intent

Remove a trivial local diagnostic-message wrapper from the TS2428 checker tests
by importing the shared helper under the existing local name.

## Scope

- `crates/tsz-checker/tests/ts2428_tests.rs`

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(ts2428_tests::)' --no-fail-fast`
