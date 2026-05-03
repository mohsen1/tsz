# [WIP] fix(checker): align overloaded constructor inference diagnostics

- **Date**: 2026-05-03
- **Branch**: `fix/generic-constructor-overload-inference-05031815`
- **PR**: #2597
- **Status**: implemented
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Investigate and fix `genericCallWithOverloadedConstructorTypedArguments.ts`,
where `tsc` reports TS2454 and TS2769, while `tsz` additionally emits TS2345
for generic calls whose argument is an overloaded construct-signature object.

## Files Touched

- `crates/tsz-solver/src/operations/constraints/signatures.rs`
  - Match overloaded source/target signatures from the bottom up in the
    many-to-many inference path, mirroring TypeScript's `inferFromSignatures`.
  - Keep the single-target overload selection path unchanged.
- `crates/tsz-checker/tests/conformance_issues/core/helpers.rs`
  - Added a focused regression for overloaded constructor callback inference.

## Verification

- `scripts/session/quick-pick.sh --run` selected and reproduced
  `TypeScript/tests/cases/conformance/types/typeRelationships/typeInference/genericCallWithOverloadedConstructorTypedArguments.ts`
  with expected `[TS2454, TS2769]` and actual `[TS2345, TS2454, TS2769]`.
- `cargo nextest run -p tsz-checker test_overloaded_constructor_callback_infers_pairwise_construct_signatures test_generic_constructor_callback_mismatch_reports_ts2345 test_generic_constructor_callback_valid_cases_stay_clean test_generic_constructor_callback_with_leading_arg`
  - 4 passed.
- `cargo build --profile dist-fast --bin tsz`
  - passed.
- `./scripts/conformance/conformance.sh run --filter "genericCallWithOverloadedConstructorTypedArguments" --verbose`
  - 2/2 passed.
- `cargo check -p tsz-checker`
  - passed.
