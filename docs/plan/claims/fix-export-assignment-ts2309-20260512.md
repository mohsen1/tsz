# fix(checker): emit TS2309 for export assignment conflicts

- **Date**: 2026-05-12
- **Branch**: `fix/export-assignment-ts2309-20260512`
- **PR**: #5869
- **Status**: ready
- **Workstream**: diagnostics correctness

## Intent

Restore the missing TS2309 diagnostic when a source file combines `export =`
with other exported elements. This closes #5841's minimal reproduction where
`tsc` reports TS1203 and TS2309 but `tsz` previously only reported TS1203.

## Files Touched

- `crates/tsz-checker/src/diagnostics.rs`
- `crates/tsz-checker/src/types/checker/ast.rs`
- `crates/tsz-checker/tests/export_assignment_tests.rs`
- `docs/plan/claims/fix-export-assignment-ts2309-20260512.md`

## Verification

- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo test -p tsz-checker export_equals_with_named_export_emits_ts2309 -- --nocapture` — passed
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target ./scripts/conformance/conformance.sh run --filter "exportAssignmentWithExports" --verbose` — 1/1 passed
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target ./scripts/conformance/conformance.sh run --filter "exportAssignmentAndDeclaration" --verbose` — 1/1 passed
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target ./scripts/conformance/conformance.sh run --filter "es6ExportEquals" --verbose` — 2/2 passed
