# chore(checker-tests): remove dead type-alias namespace helper

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-type-alias-namespace-helper-20260512`
- **Base**: `origin/main`
- **Issue**: n/a
- **PR**: tbd
- **Status**: claim
- **Labels**: `DRY`, `checker-tests`

## Intent

Remove an unused diagnostic-message helper and its `#[allow(dead_code)]` escape
hatch from the type-alias namespace merge tests.

## Scope

- Delete the unused `get_diagnostics` helper in
  `crates/tsz-checker/tests/type_alias_namespace_merge_tests.rs`.

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --test type_alias_namespace_merge_tests --no-fail-fast`
