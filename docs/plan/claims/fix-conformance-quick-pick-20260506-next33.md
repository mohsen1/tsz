# fix(checker): preserve rest tuple return-context inference

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-quick-pick-20260506-next33`
- **PR**: #3910
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Claiming `TypeScript/tests/cases/compiler/contextualParamTypeVsNestedReturnTypeInference4.ts`.

Current `origin/main` emits one extra `TS2322` where tsc accepts nested contextual return inference through `effectGen` / `effectFn` generator callbacks.

The fix keeps rest-to-rest contextual return inference flat, so `Args` is seeded as `[string]` instead of `[[string]]` when a nested generic generator callback is checked against a contextual rest-tuple return signature.

## Files Touched

- `crates/tsz-solver/src/operations/generic_call/return_context.rs`
- `crates/tsz-checker/src/tests/dispatch_tests.rs`

## Verification

- Baseline: `./scripts/conformance/conformance.sh run --filter "contextualParamTypeVsNestedReturnTypeInference4" --verbose` (extra TS2322)
- `CARGO_TARGET_DIR=.target/nextest-local cargo nextest run -p tsz-checker nested_return_context_rest_tuple_callback_args_are_not_wrapped`
- `./scripts/conformance/conformance.sh run --filter "contextualParamTypeVsNestedReturnTypeInference4" --verbose` (1/1 passed)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
- `git diff --check`
- `scripts/architecture-check.sh --quick`
- `CARGO_TARGET_DIR=.target/nextest-local cargo clippy -p tsz-checker --lib -- -D warnings`
