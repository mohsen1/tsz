# fix(checker): align instantiation expression fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-instantiation-expressions-fingerprint`
- **PR**: #2814
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only divergence in
`TypeScript/tests/cases/conformance/types/typeParameters/typeArgumentLists/instantiationExpressions.ts`.
The picker reports matching diagnostic codes (`TS1099`, `TS2344`, `TS2635`),
so this PR will root-cause the remaining message, display, count, or anchor
mismatch around instantiation expression diagnostics.

## Files Touched

- `docs/plan/claims/fix-checker-instantiation-expressions-fingerprint.md`
  (claim)
- `crates/tsz-checker/src/dispatch.rs`
- `crates/tsz-checker/src/state/type_resolution/constructors.rs`
- `crates/tsz-checker/src/types/type_node_advanced.rs`
- `crates/tsz-checker/src/types/type_checking/type_alias_checking.rs`
- `crates/tsz-checker/src/checkers/generic_checker/constraint_validation.rs`
- `crates/tsz-checker/src/error_reporter/generics.rs`
- `crates/tsz-checker/src/state/type_environment/formatting.rs`
- `crates/tsz-checker/src/tests/dispatch_tests.rs`

## Verification

- `cargo check --package tsz-checker`
- `cargo test -p tsz-checker dispatch_tests -- --nocapture`
- `./scripts/conformance/conformance.sh run --filter "instantiationExpressions" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/githooks/pre-commit`
