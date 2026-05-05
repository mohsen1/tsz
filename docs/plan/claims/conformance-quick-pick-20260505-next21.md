# [WIP] fix(checker): align intra-expression inference diagnostic surface

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-20260505-next21`
- **PR**: #3308
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the quick-picked fingerprint-only mismatch in
`intraExpressionInferences.ts`. The diagnostic code set already matches tsc
(`TS2322`, `TS2339`), but the `map` function assignment diagnostic around
`test.ts:132:5` reports an `error` return property and an optional function
target where tsc reports `any` and the direct function target surface.

## Files Touched

- `crates/tsz-checker/src/error_reporter/call_errors/elaboration.rs`
- `crates/tsz-checker/src/types/computation/intra_expression_inference_tests.rs`
- `crates/tsz-checker/tests/strict_null_manual.rs`
- `crates/tsz-solver/src/type_queries/data/signatures_and_advanced.rs`
- `crates/tsz-solver/src/tests/type_queries_function_rewrite_tests.rs`

## Verification

- `scripts/session/quick-pick.sh --run` selected and reproduced
  `TypeScript/tests/cases/conformance/types/typeRelationships/typeInference/intraExpressionInferences.ts`.
- `cargo fmt --check`
- `cargo nextest run -p tsz-solver rewrite_function_error_slots_to_any --failure-output immediate-final --no-fail-fast`
- `CARGO_BUILD_JOBS=1 cargo nextest run -p tsz-checker --lib strict_null_manual::test_optional_property_error_message_with_strict_null_checks --failure-output immediate-final --no-fail-fast`
- `cargo nextest run -p tsz-checker intra_expression_inference_homomorphic_mapped_return --failure-output immediate-final --no-fail-fast`
  passed before the final recursive-cache hardening; later reruns were blocked by local `.target/debug` artifact churn and SIGTERM during unrelated/package test compilation.
- `./scripts/conformance/conformance.sh run --filter "intraExpressionInferences" --verbose`
  passed after the final code change: `FINAL RESULTS: 2/2 passed (100.0%)`, `Fingerprint-only: 0`.
