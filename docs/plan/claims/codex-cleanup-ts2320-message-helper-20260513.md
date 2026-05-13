# chore(checker-tests): reuse TS2320 diagnostic helper

- **Date**: 2026-05-13
- **Branch**: `codex/cleanup-ts2320-message-helper-20260513`
- **Base**: `origin/main`
- **Issue**: n/a
- **PR**: [#6090](https://github.com/mohsen1/tsz/pull/6090)
- **Status**: ready
- **Labels**: `DRY`, `checker-tests`

## Intent

Remove a trivial local diagnostic-message wrapper from the TS2320 checker tests
by importing the shared helper under the existing local name.

## Scope

- `crates/tsz-checker/tests/ts2320_tests.rs`

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(ts2320_tests::)' --no-fail-fast`
