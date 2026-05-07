# fix(checker): align generic construct signature TS2430 fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-next-20260506-144453`
- **PR**: #4156
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Target the quick-pick conformance failure:
`TypeScript/tests/cases/conformance/types/typeRelationships/subtypesAndSuperTypes/subtypingWithGenericConstructSignaturesWithOptionalParameters.ts`.

`tsc` and tsz agree on diagnostic code `TS2430`, but the fingerprints differ
for generic construct signature inheritance failures. This slice will diagnose
whether the drift is diagnostic anchoring, message shaping, or generic
signature comparison behavior, then align the fingerprints without weakening
the shared inheritance relation path.

## Files Touched

- `crates/tsz-checker/src/query_boundaries/class.rs`
- `crates/tsz-checker/tests/ts2430_tests.rs`

## Verification

- `cargo fmt --check`
- `CARGO_BUILD_JOBS=2 cargo check -p tsz-checker --lib`
- `CARGO_BUILD_JOBS=2 cargo check -p tsz-solver --lib`
- `CARGO_BUILD_JOBS=2 cargo nextest run -p tsz-checker -E 'test(test_constructor_typed_property_with_outer_type_param_errors)'`
- `CARGO_BUILD_JOBS=2 cargo nextest run -p tsz-checker -E 'test(ts2430_tests::)'`
- `./scripts/arch/check-checker-boundaries.sh`
- `CARGO_BUILD_JOBS=2 cargo clippy -p tsz-checker --all-targets -- -D warnings`
- `CARGO_BUILD_JOBS=2 ./scripts/conformance/conformance.sh run --filter "subtypingWithGenericConstructSignaturesWithOptionalParameters" --verbose`
  - Result: `1/1 passed`, no fingerprint-only failures.
- `CARGO_BUILD_JOBS=2 ./scripts/conformance/conformance.sh run --max 200 --print-fingerprints`
  - Result: `197/200 passed`.
  - Known/unrelated smoke failures:
    - `TypeScript/tests/cases/compiler/anyIndexedAccessArrayNoException.ts` (`TS2538` column drift)
    - `TypeScript/tests/cases/compiler/accessorWithoutBody1.ts` (extra parser `TS1005`)
    - `TypeScript/tests/cases/compiler/accessorWithoutBody2.ts` (extra parser `TS1005`)
