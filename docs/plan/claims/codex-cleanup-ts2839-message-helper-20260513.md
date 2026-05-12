# chore(checker-tests): reuse TS2839 diagnostic helper

- **Date**: 2026-05-13
- **Branch**: `codex/cleanup-ts2839-message-helper-20260513`
- **Base**: `origin/main`
- **Issue**: n/a
- **PR**: tbd
- **Status**: claim
- **Labels**: `DRY`, `checker-tests`

## Intent

Remove a trivial local diagnostic-message wrapper from the TS2839 checker tests
by importing the shared helper under the existing local name.

## Scope

- `crates/tsz-checker/tests/ts2839_tests.rs`

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(ts2839_tests::)' --no-fail-fast`
