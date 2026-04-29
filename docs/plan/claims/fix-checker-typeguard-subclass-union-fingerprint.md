# fix(checker): preserve subclass union members in type-predicate narrowing

- **Date**: 2026-04-28
- **Branch**: `fix/checker-typeguard-subclass-union-fingerprint`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only mismatch in
`TypeScript/tests/cases/conformance/expressions/typeGuards/typeGuardOfFormIsType.ts`.
The slice targets user-defined type-predicate narrowing for class union members,
where a subclass that satisfies the predicate should remain the subclass rather
than falling back to a redundant intersection display.

## Files Touched

- `crates/tsz-solver/src/narrowing/core.rs`
- `crates/tsz-checker/tests/control_flow_type_guard_tests.rs`
- `docs/plan/claims/fix-checker-typeguard-subclass-union-fingerprint.md`

## Verification

- `cargo nextest run -p tsz-checker type_predicate_preserves_subclass_union_member_without_redundant_intersection`
- `cargo check --package tsz-solver`
- `cargo fmt --check --package tsz-solver --package tsz-checker` (blocked by pre-existing formatting drift in `crates/tsz-checker/src/state/type_resolution/module.rs`)
- `cargo build --target-dir .target --profile dist-fast -p tsz-cli -p tsz-conformance`
- Target conformance `typeGuardOfFormIsType` passed 2/2 with no fingerprint-only mismatch
- Direct max-200 smoke passed 199/200 with only pre-existing `aliasOnMergedModuleInterface.ts` TS2708 failure
