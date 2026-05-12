# chore(checker-tests): reuse TS2838 diagnostic message helper

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-ts2838-message-helper-20260512`
- **Base**: `origin/main`
- **Issue**: n/a
- **PR**: [#6000](https://github.com/mohsen1/tsz/pull/6000)
- **Status**: ready
- **Labels**: `DRY`, `checker-tests`

## Intent

Remove a trivial local diagnostic-message wrapper from the TS2838 checker tests
by importing the shared helper under the existing local name.

## Scope

- `crates/tsz-checker/tests/ts2838_tests.rs`

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(ts2838_tests::)' --no-fail-fast`
