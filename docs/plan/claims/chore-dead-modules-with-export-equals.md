# chore(binder): remove dead `modules_with_export_equals` field

- **Date**: 2026-05-02
- **Branch**: `chore/dead-modules-with-export-equals`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 8 (DRY cleanup)

## Intent

`BinderState::modules_with_export_equals: FxHashSet<String>` was added
intending to track which modules have an `export = X` statement, but is
never written anywhere in the codebase — `grep -rn modules_with_export
_equals\\.\\(insert\\|extend\\|push\\|set\\)` returns zero hits across
all crates. The single reader in `export_surface.rs` (line 204) calls
`.contains(file_name)` against this perpetually-empty set, so
`surface.has_export_equals` is always false from `from_binder()`. No
production code consumes that surface flag, so the bug was latent.

This change:

1. Removes the dead `modules_with_export_equals` field from
   `BinderState` and `BinderStateScopeInputs` and every initializer
   site (CLI driver, parallel core legacy/shared, tsz-cli driver
   tests, tsz-core parallel tests).
2. Repoints `ExportSurface::from_binder()` to compute
   `has_export_equals` from the binder's `module_exports` table —
   `module_exports.get(file_name).is_some_and(|exports|
   exports.has("export="))` — which uses the populated source-of-truth
   the rest of the codebase already keys off.

The `.has_export_equals` field on `ExportSurface` is preserved (only
the source of its value changes) so the small test that exercises
clone preservation still validates the structural invariant.

## Files Touched

- `crates/tsz-binder/src/state/mod.rs` (~3 LOC)
- `crates/tsz-binder/src/state/core.rs` (~3 LOC)
- `crates/tsz-binder/src/state/export_surface.rs` (~10 LOC, fixed
  population path)
- `crates/tsz-cli/src/driver/check_utils.rs` (~2 LOC)
- `crates/tsz-cli/tests/driver_tests.rs` (~1 LOC)
- `crates/tsz-core/src/parallel/core.rs` (~2 LOC)
- `crates/tsz-core/tests/parallel_tests.rs` (~1 LOC)

## Verification

- `cargo nextest run -p tsz-binder` — 452 passed
- `cargo nextest run -p tsz-core` — full suite passes
- `cargo check --workspace --tests` — clean
