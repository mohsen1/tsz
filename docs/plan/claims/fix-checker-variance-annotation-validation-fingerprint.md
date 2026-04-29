# fix(checker): align variance annotation validation fingerprints

- **Date**: 2026-04-29
- **Branch**: `fix/checker-variance-annotation-validation-fingerprint`
- **PR**: #1747
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

This PR targets the random conformance pick `TypeScript/tests/cases/compiler/varianceAnnotationValidation.ts`.
The root cause was that TSZ used computed structural variance for both TS2636 validation and generic
application assignability; TypeScript validates invalid declared annotations against actual usage, but still
uses the declared `in`/`out` direction when checking assignments. The fix separates declared variance from
actual structural variance while keeping the checker surface behind query-boundary helpers.

## Files Touched

- `crates/tsz-checker/src/context/resolver.rs`
- `crates/tsz-checker/src/context/def_mapping.rs`
- `crates/tsz-checker/src/query_boundaries/variance.rs`
- `crates/tsz-checker/src/state/state_checking_members/interface_checks.rs`
- `crates/tsz-checker/tests/conformance_issues/core/helpers.rs`
- `crates/tsz-solver/src/def/resolver.rs`
- `crates/tsz-solver/src/relations/relation_queries.rs`
- `crates/tsz-solver/src/relations/subtype/rules/generics.rs`
- `crates/tsz-solver/src/relations/variance.rs`
- `docs/plan/claims/fix-checker-variance-annotation-validation-fingerprint.md`

## Verification

- `cargo fmt --check`
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run --package tsz-solver --lib` (5545 passed, 9 skipped)
- `cargo nextest run --package tsz-checker --lib` (2964 passed, 11 skipped)
- `cargo nextest run --package tsz-checker test_declared_out_variance_controls_application_assignability_even_when_invalid` (1 passed)
- `./scripts/conformance/conformance.sh run --filter "varianceAnnotationValidation" --verbose` (1/1 passed)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run` (FINAL RESULTS: 12238/12582 passed, 97.3%)
