# fix(checker): align mapped indexed access TS2322 fingerprint

- **Date**: 2026-04-29
- **Branch**: `fix/mapped-type-indexed-access-fingerprint`
- **PR**: #1816
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance and fingerprints)

## Intent

`scripts/session/quick-pick.sh` selected `TypeScript/tests/cases/compiler/mappedTypeIndexedAccess.ts`, a fingerprint-only TS2322 mismatch. The root cause was that object-literal elaboration kept drilling into the discriminant-selected union member for mapped indexed-access targets, while `tsc` reports the outer assignment against the indexed-access surface. This PR suppresses that property elaboration only for indexed-access target surfaces and formats concrete `...[keyof ...]` assignment annotations with the evaluated union display that `tsc` uses.

## Files Touched

- `crates/tsz-checker/src/error_reporter/call_errors/elaboration.rs`
- `crates/tsz-checker/src/error_reporter/core/diagnostic_source/assignment_formatting.rs`
- `crates/tsz-checker/src/lib.rs`
- `crates/tsz-checker/tests/mapped_indexed_access_diagnostic_tests.rs`
- `docs/plan/claims/fix-mapped-type-indexed-access-fingerprint.md`

## Verification

- `cargo fmt --check`
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run --package tsz-checker --lib mapped_indexed_access_discriminated_union_reports_outer_assignment discriminated_union_object_literal_reports_matching_member_property_mismatch` (2 passed)
- `cargo nextest run --package tsz-checker --lib` (3008 passed, 10 skipped)
- `./scripts/conformance/conformance.sh run --filter "mappedTypeIndexedAccess" --verbose` (2/2 passed)
- `./scripts/conformance/conformance.sh run --filter "nonPrimitiveConstraintOfIndexAccessType" --verbose` (1/1 passed; regression guard for generic `T[P]` display)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run` (12249/12582 passed, 97.4%; net +14, 42 improvements including `mappedTypeIndexedAccess.ts`, 28 PASS->FAIL entries reported by the current snapshot comparison)
