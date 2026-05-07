# fix(checker): align deeply nested mapped type fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-quick-pick-20260506-next25`
- **PR**: #3819
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Claiming `TypeScript/tests/cases/compiler/deeplyNestedMappedTypes.ts`.

Current `origin/main` emits `TS2322`, but the fingerprints differ from tsc:

- Missing `TS2322` at `test.ts:18:7` for the nested `Id2<...>` assignment.
- Missing `TS2322` at `test.ts:70:5`, `test.ts:74:5`, and `test.ts:78:5` where `Input[]` should display as its expanded object-array type.
- Extra `TS2322` at `test.ts:74:5` using the alias display `Input[]`.

This slice will align assignment diagnostic source display for deeply nested
mapped/static schema types without changing the TS2322 code surface.

## Files Touched

- `crates/tsz-checker/src/assignability/assignability_checker.rs`
- `crates/tsz-checker/src/assignability/assignment_checker_tests.rs`
- `crates/tsz-checker/src/error_reporter/assignability.rs`
- `crates/tsz-checker/src/error_reporter/core/diagnostic_source.rs`
- `crates/tsz-checker/src/error_reporter/core/diagnostic_source/assignment_formatting.rs`
- `crates/tsz-checker/src/error_reporter/render_failure/type_mismatch.rs`
- `crates/tsz-checker/src/types/type_node_advanced.rs`

## Verification

- Baseline: `./scripts/conformance/conformance.sh run --filter "deeplyNestedMappedTypes" --verbose` (fingerprint-only failure)
- `./scripts/conformance/conformance.sh run --filter "deeplyNestedMappedTypes" --verbose` (1/1 passed)
- `CARGO_TARGET_DIR=.target/nextest-local cargo nextest run -p tsz-checker recursive_mapped_alias_application_display_stays_at_application typebox_static_array_return_diagnostics_use_structural_display`
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
- `git diff --check`
- `scripts/architecture-check.sh --quick` (exits 0 with existing LOC warnings)
- `CARGO_TARGET_DIR=.target/nextest-local cargo clippy -p tsz-checker --lib -- -D warnings`
