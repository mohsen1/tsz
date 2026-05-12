# chore(checker-tests): reuse TS2411 diagnostic helper

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-ts2411-message-helper-20260512`
- **Base**: `origin/main`
- **Issue**: n/a
- **PR**: [#6039](https://github.com/mohsen1/tsz/pull/6039)
- **Status**: ready
- **Labels**: `DRY`, `checker-tests`

## Intent

Remove a trivial local diagnostic-message wrapper from the TS2411 checker tests
by importing the shared helper under the existing local name.

## Scope

- `crates/tsz-checker/tests/ts2411_tests.rs`

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(ts2411_tests::)' --no-fail-fast`
