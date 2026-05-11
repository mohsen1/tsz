# fix(checker): align assignmentCompatWithCallSignatures4 fingerprints

- **Date**: 2026-05-12
- **Branch**: `fix/assignment-compat-call-signatures4-fingerprint-20260512`
- **PR**: https://github.com/mohsen1/tsz/pull/5652
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Close the fingerprint-only conformance failure for
`TypeScript/tests/cases/conformance/types/typeRelationships/assignmentCompatibility/assignmentCompatWithCallSignatures4.ts`.
The current dashboard reports matching diagnostic codes but TS2322/TS2564
fingerprint drift.

## Files Touched

- `crates/tsz-checker/src/error_reporter/core/diagnostic_source.rs`
- `crates/tsz-checker/tests/ts2322_literal_source_display_tests.rs`
- `docs/plan/claims/fix-assignment-compat-call-signatures4-fingerprint-20260512.md`

## Verification

- Baseline: `scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter assignmentCompatWithCallSignatures4 --verbose` (0/1 passed, fingerprint-only; `{ foo: number;; }` vs `{ foo: number; }`)
- `cargo fmt --all --check` (passed)
- `cargo test -p tsz-checker --test ts2322_literal_source_display_tests ts2322_function_type_parameter_object_display_has_single_trailing_semicolon` (passed)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter assignmentCompatWithCallSignatures4 --verbose` (1/1 passed, fingerprint-only 0)
- `git diff --check` (passed)
