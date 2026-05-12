# chore(checker-tests): inline destructuring rest diagnostic helper

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-destructuring-rest-message-helper-20260512`
- **Base**: `origin/main`
- **Issue**: n/a
- **PR**: [#5977](https://github.com/mohsen1/tsz/pull/5977)
- **Status**: ready
- **Labels**: `DRY`, `checker-tests`

## Intent

Remove a trivial local passthrough diagnostic wrapper in destructuring-rest
tests by importing the existing checker diagnostic message helper directly.

## Scope

- Migrate `crates/tsz-checker/tests/destructuring_rest_omit_unspreadable_tests.rs`.

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --test destructuring_rest_omit_unspreadable_tests --no-fail-fast`
