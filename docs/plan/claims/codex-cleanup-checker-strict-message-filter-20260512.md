# chore(checker-tests): share strict diagnostic message filter

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-checker-strict-message-filter-20260512`
- **Base**: `origin/main`
- **Issue**: n/a
- **PR**: [#5964](https://github.com/mohsen1/tsz/pull/5964)
- **Status**: ready
- **Labels**: `DRY`, `checker-tests`

## Intent

Remove a local checker-test helper that reimplements strict diagnostic message
projection and manually filters missing-default-lib noise.

## Scope

- Add a shared strict `(code, message_text)` helper that excludes TS2318.
- Migrate `crates/tsz-checker/tests/generic_return_fingerprint_tests.rs`.

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --test generic_return_fingerprint_tests --no-fail-fast`
- `cargo clippy -p tsz-checker --lib -- -D warnings`
