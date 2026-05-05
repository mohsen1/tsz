# fix(checker): align generic construct signature optional parameter diagnostic fingerprint

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-20260505-next2`
- **PR**: #2751
- **Status**: ready
- **Workstream**: 1 (Conformance fixes)

## Intent

This PR targets the fingerprint-only failure in
`TypeScript/tests/cases/conformance/types/typeRelationships/subtypesAndSuperTypes/subtypingWithGenericConstructSignaturesWithOptionalParameters.ts`.
Both `tsc` and `tsz` emit `TS2430`; the remaining gap is the exact diagnostic
fingerprint.

The missing fingerprint is the `I6` interface extension diagnostic. `tsc`
rejects the derived `new <T>(x: T) => T` construct property against the base
`new <T>(x?: T) => T` construct property because the optional generic target
parameter can make the source return effectively `T | undefined`, which is not
assignable to the required `T` return. `tsz` accepted that member because the
same-arity generic construct signatures were alpha-renamed before this optional
target parameter relationship could force the return mismatch.

## Files Touched

- `crates/tsz-checker/src/query_boundaries/class.rs`
- `crates/tsz-checker/tests/ts2430_tests.rs`

## Verification

- `cargo nextest run -p tsz-checker --lib -E 'test(test_generic_construct_property_required_param_against_optional_base_errors)'` passed
- `cargo nextest run -p tsz-checker --lib -E 'test(/ts2430_tests::/)'` passed
- `./scripts/conformance/conformance.sh run --filter "subtypingWithGenericConstructSignaturesWithOptionalParameters" --verbose` passed, `FINAL RESULTS: 1/1 passed (100.0%)`
- `cargo check --package tsz-checker && cargo check --package tsz-solver` passed
- `cargo nextest run --package tsz-checker --lib` passed, `3333/3333 passed, 10 skipped`
- `cargo nextest run --package tsz-solver --lib` passed, `5622/5622 passed, 9 skipped`
- `cargo build --profile dist-fast --bin tsz && ./scripts/conformance/conformance.sh run --max 200` passed, `FINAL RESULTS: 200/200 passed (100.0%)`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL` passed, `FINAL RESULTS: 12438/12582 passed (98.9%)`
