# fix(checker): align type-star value collision diagnostics

- **Date**: 2026-04-28
- **Branch**: `fix/export-type-star-value-collision-fingerprint`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance fingerprints)

## Intent

Fix the fingerprint-only mismatch in `exportNamespace9.ts` where `export type *`
collides with a value wildcard export. The target behavior is to keep the
TS2308 ambiguity diagnostic anchored on the type-only wildcard export while
preserving the downstream TS2749 value-vs-type diagnostic in consumers.

## Files Touched

- `crates/tsz-checker/src/declarations/import/exports.rs` (wildcard collision diagnostic anchor)
- `crates/tsz-checker/tests/conformance_issues/modules/declaration_module_emit.rs` (regression coverage)

## Verification

- `cargo check -p tsz-checker`
- `cargo fmt --check --package tsz-checker`
- `git diff --check`
- `CARGO_TARGET_DIR=.target cargo nextest run -p tsz-checker test_export_type_star_collides_with_value_star_reexport`
- `./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --filter exportNamespace9 --verbose --workers 1`
- `./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --max 200 --workers 1` (199/200; existing `aliasOnMergedModuleInterface.ts` TS2708 miss)
