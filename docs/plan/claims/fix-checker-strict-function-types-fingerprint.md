# [WIP] fix(checker): align strict function type fingerprints

- **Date**: 2026-04-29
- **Branch**: `fix/checker-strict-function-types-fingerprint`
- **PR**: #1724
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance / fingerprint-only TS2322 TS2328)

## Intent

Fix the fingerprint-only divergence in
`TypeScript/tests/cases/compiler/strictFunctionTypesErrors.ts`, selected by
`scripts/session/quick-pick.sh`. The initial target has matching diagnostic
codes (`TS2322`, `TS2328`) but mismatched fingerprints, so the investigation
will focus on message rendering, elaboration, and/or diagnostic anchoring
while keeping assignability routed through the shared checker/solver boundary.

## Files Touched

- `crates/tsz-solver/src/relations/variance.rs`
- `crates/tsz-checker/tests/signature_assignability_regression_tests.rs`
- `docs/plan/claims/fix-checker-strict-function-types-fingerprint.md`

## Verification

- `cargo check -p tsz-solver` (passes)
- `cargo nextest run -p tsz-checker --test signature_assignability_regression_tests method_only_generic_variance_is_bivariant` (passes)
- `./scripts/conformance/conformance.sh run --filter "strictFunctionTypesErrors" --verbose` (still fingerprint-only; the incorrect `Comparer1<Animal>` / `Comparer1<Dog>` extra TS2322 is fixed, but nested `Func<Func<...>>` and extracted-method callback fingerprints remain)
