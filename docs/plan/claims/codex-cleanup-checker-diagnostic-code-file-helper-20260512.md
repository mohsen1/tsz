# chore(checker-tests): share named-file diagnostic code helper

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-checker-diagnostic-code-file-helper-20260512`
- **Base**: `origin/main`
- **Issue**: n/a
- **PR**: [#5962](https://github.com/mohsen1/tsz/pull/5962)
- **Status**: ready
- **Labels**: `DRY`, `checker-tests`

## Intent

Remove another small checker-test helper that locally maps diagnostics to codes
for a named source file. Keep the test behavior unchanged while routing the
common parse/bind/check path through `tsz_checker::test_utils`.

## Scope

- Add a canonical code-only named-file helper in `crates/tsz-checker/src/test_utils.rs`.
- Migrate `crates/tsz-checker/tests/ambient_default_namespace_export_dup_tests.rs`
  away from its local duplicate wrapper.

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --test ambient_default_namespace_export_dup_tests --no-fail-fast`
- `cargo clippy -p tsz-checker --lib -- -D warnings`
