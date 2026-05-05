# fix(checker): suppress extra TS2769 for generic return inference

- **Date**: 2026-05-05
- **Branch**: `fix/checker-infer-generic-return-extra-ts2769`
- **PR**: #2867
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the only-extra diagnostic divergence in
`TypeScript/tests/cases/compiler/inferFromGenericFunctionReturnTypes3.ts`.
The picker reports expected `TS2322,TS2345` and actual
`TS2322,TS2345,TS2769`, so this PR will root-cause and remove the extra
overload diagnostic without losing the expected assignability diagnostics.

## Files Touched

- `docs/plan/claims/fix-checker-infer-generic-return-extra-ts2769.md`
  (claim)
- `crates/tsz-solver/src/relations/subtype/rules/tuples.rs`
- `crates/tsz-solver/src/tests/visitor_tests.rs`

## Verification

- `cargo check --package tsz-solver`
- `cargo check --package tsz-checker`
- `cargo test -p tsz-solver any_array_assignable_to_unknown_entry_tuple_for_map_inference -- --nocapture`
- `./scripts/conformance/conformance.sh run --filter "inferFromGenericFunctionReturnTypes3" --verbose`
  - TS2769 code mismatch fixed; case is now fingerprint-only on existing
    literal/contextual-inference diagnostics.
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/githooks/pre-commit`
