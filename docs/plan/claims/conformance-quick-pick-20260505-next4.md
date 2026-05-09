# fix(checker): preserve tuple literal inference from tuple assertions

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-20260505-next4`
- **PR**: #2769
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the conformance fingerprint mismatch in `typeInferenceWithTupleType.ts`.
The picked failure has matching diagnostic codes, but tsz emits extra TS2322
fingerprints for `expected = f1(undefined as ["a"[], "b"[]])` and the readonly
tuple overload because tuple inference widens the inferred `T1` to `string`
where `tsc` preserves the literal `"a"`.

## Files Touched

- `crates/tsz-solver/src/operations/core/call_evaluator.rs`
- `crates/tsz-solver/src/operations/core/call_resolution.rs`
- `crates/tsz-solver/src/operations/generic_call/resolve.rs`
- `crates/tsz-solver/src/operations/mod.rs`
- `crates/tsz-checker/src/query_boundaries/checkers/call.rs`
- `crates/tsz-checker/src/checkers/call_checker/applicability.rs`
- `crates/tsz-checker/src/checkers/call_checker/candidate_collection.rs`
- `crates/tsz-checker/src/types/computation/call/mod.rs`
- `crates/tsz-checker/src/types/computation/call/inner.rs`
- `crates/tsz-checker/Cargo.toml`
- `crates/tsz-checker/tests/tuple_type_assertion_inference_tests.rs`

## Verification

- `scripts/session/quick-pick.sh --run` selected and reproduced
  `TypeScript/tests/cases/conformance/types/tuple/typeInferenceWithTupleType.ts`.
- `cargo nextest run -p tsz-checker --test tuple_type_assertion_inference_tests`
- `./scripts/conformance/conformance.sh run --filter "typeInferenceWithTupleType" --verbose`
  => `FINAL RESULTS: 1/1 passed (100.0%)`, `Fingerprint-only: 0`.
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `./scripts/conformance/conformance.sh run --max 200`
  => `FINAL RESULTS: 200/200 passed (100.0%)`.
- `./scripts/conformance/conformance.sh run`
  => `FINAL RESULTS: 12440/12582 passed (98.9%)`, net `+3`, including
  `typeInferenceWithTupleType.ts` as an improvement. The run reports three
  PASS -> FAIL baseline deltas (`dynamicNames.ts`,
  `jsDeclarationsTypeAliases.ts`, `typedefTagTypeResolution.ts`); each
  reproduces with this patch stashed.
- `git diff --check`
