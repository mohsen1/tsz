# fix(checker): align prototype assignment TS2339 fingerprint

- **Date**: 2026-05-01
- **Branch**: `fix/checker-type-from-prototype-assignment-fingerprint`
- **PR**: #2710
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Investigate and fix the fingerprint-only TS2339 mismatch in
`TypeScript/tests/cases/conformance/salsa/typeFromPrototypeAssignment.ts`.
The work will preserve the shared property-access diagnostic path and add an
owning-crate regression test for the structural rule behind the mismatch.

## Files Touched

- `docs/plan/claims/fix-checker-type-from-prototype-assignment-fingerprint.md`
- `crates/tsz-checker/Cargo.toml`
- `crates/tsz-checker/src/state/state_checking_members/ambient_signature_checks.rs`
- `crates/tsz-checker/src/types/computation/complex_constructors.rs`
- `crates/tsz-checker/src/types/computation/complex_js_constructor.rs`
- `crates/tsz-checker/src/types/computation/object_literal/computation.rs`
- `crates/tsz-checker/src/types/function_type.rs`
- `crates/tsz-checker/src/types/function_type/js_prototype.rs`
- `crates/tsz-checker/tests/jsdoc_prototype_assignment_target_display.rs`

## Verification

- `cargo fmt`
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo nextest run -p tsz-checker --test jsdoc_prototype_assignment_target_display`
- `./scripts/conformance/conformance.sh run --filter "typeFromPrototypeAssignment" --verbose`
  - The targeted `typeFromPrototypeAssignment.ts` file now passes.
  - The broader filter still reports the existing nested
    `typeFromPrototypeAssignment2.ts` fingerprint-only mismatch.
- `./scripts/conformance/conformance.sh run --max 200` (200/200)
