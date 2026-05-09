# fix(solver): align recursive generic call signature assignment

- **Date**: 2026-05-06
- **Branch**: `fix/assignment-compat-generic-call-signatures4-20260506`
- **PR**: #3705
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the quick-picked fingerprint-only target
`TypeScript/tests/cases/conformance/types/typeRelationships/assignmentCompatibility/assignmentCompatWithGenericCallSignatures4.ts`.
The expected and actual code sets both contain `TS2322`, so this slice is
scoped to root-causing the diagnostic message or location drift in generic
call signature assignment compatibility and landing the fix in the owning
checker/solver path with focused Rust coverage.

## Files Touched

- `docs/plan/claims/fix-assignment-compat-generic-call-signatures4-20260506.md`
- `crates/tsz-solver/src/relations/subtype/rules/functions/checking.rs`
- `crates/tsz-checker/tests/ts2322_tests.rs`
- `crates/tsz-cli/tests/driver_tests.rs`

## Verification

- `cargo fmt --check`
- `CARGO_BUILD_JOBS=2 cargo nextest run -p tsz-checker -p tsz-cli recursive_generic_signature_assignment_reports_only_tsc_direction compile_recursive_generic_signature_assignment_reports_only_tsc_direction generic_signature_assignment_reports_expected_ts2322s`
- `CARGO_BUILD_JOBS=2 ./scripts/conformance/conformance.sh run --filter "assignmentCompatWithGenericCallSignatures4" --verbose`
