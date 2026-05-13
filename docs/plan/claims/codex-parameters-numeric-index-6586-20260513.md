# fix(solver): allow numeric index on Parameters<T>

- **Date**: 2026-05-13
- **Branch**: `codex/parameters-numeric-index-6586-20260513`
- **PR**: #6610
- **Status**: ready
- **Workstream**: conformance / solver false positives

## Intent

Fix #6586 so indexed access like `Parameters<T>[0]` works when `T` is a
generic function type parameter constrained to a callable. This should remove
the false TS2536 while preserving real invalid indexed-access diagnostics.

## Files Touched

- `crates/tsz-checker/src/types/type_checking/indexed_access.rs`
- `crates/tsz-checker/tests/tuple_index_access_tests.rs`
- `docs/plan/claims/codex-parameters-numeric-index-6586-20260513.md`

## Verification

- `cargo test -p tsz-checker --test tuple_index_access_tests parameters_of_generic_function_allows_numeric_index -- --nocapture` (1 passed)
- `cargo test -p tsz-checker --test tuple_index_access_tests -- --nocapture` (15 passed)
- `cargo fmt --all --check`
