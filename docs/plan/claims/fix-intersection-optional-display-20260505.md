# fix(checker): preserve optional-property conflicts in intersections

- **Date**: 2026-05-05
- **Branch**: `fix-intersection-optional-display-20260505`
- **PR**: https://github.com/mohsen1/tsz/pull/2784
- **Status**: ready
- **Workstream**: conformance fingerprint parity

## Intent

Fix the fingerprint-only TS2322 divergence in
`TypeScript/tests/cases/compiler/intersectionsAndOptionalProperties.ts`.

Root cause was not just display: source-intersection assignability could let one
object member satisfy an object-like target while hiding a conflicting optional
property from a sibling member. The diagnostic surface also needed the tsc split:
collapse anonymous object intersections for direct assignability display, but
preserve a declared intersection annotation when that is the source expression.

## Files Touched

- `crates/tsz-solver/src/relations/subtype/core.rs`
- `crates/tsz-solver/tests/intersection_optional_subtype_tests.rs`
- `crates/tsz-checker/src/error_reporter/assignability.rs`
- `crates/tsz-checker/src/error_reporter/core/intersection_optional_display.rs`
- `crates/tsz-checker/src/error_reporter/core/diagnostic_source.rs`
- `crates/tsz-checker/src/error_reporter/core_formatting.rs`
- `crates/tsz-checker/src/query_boundaries/intersection_display.rs`
- `crates/tsz-checker/tests/ts2322_tests.rs`

## Verification

- `cargo fmt --all`
- `cargo nextest run -p tsz-checker test_ts2322_intersections_and_optional_properties_source_display test_ts2322_reports_alias_intersection_optional_property_conflict architecture_contract_tests_src::test_checker_file_size_ceiling architecture_contract_tests_src::test_solver_imports_go_through_query_boundaries`
- `cargo nextest run -p tsz-solver test_intersection_member_shortcut_preserves_optional_property_conflict`
- `cargo nextest run --package tsz-solver --lib`
- `cargo nextest run --package tsz-checker --lib`
- `./scripts/conformance/conformance.sh run --filter "intersectionsAndOptionalProperties" --verbose` — `3/3 passed`
- `./scripts/conformance/conformance.sh run --max 200` — `200/200 passed`
- `./scripts/conformance/conformance.sh run` — `12453/12582 passed`; net `12451 -> 12453` (`+2`)

The full conformance diff reports one PASS -> FAIL
(`nestedRecursiveArraysOrObjectsError01.ts`), but the same fingerprint mismatch
reproduces on a fresh current `origin/main` worktree, so it is stale baseline
drift rather than this patch.
