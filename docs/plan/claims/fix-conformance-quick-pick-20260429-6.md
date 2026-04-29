# [WIP] fix(checker): suppress extra TS2532 for recursive implicit constructor

- **Date**: 2026-04-29
- **Branch**: `fix/conformance-quick-pick-20260429-6`
- **PR**: #1815
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

The conformance picker selected `TypeScript/tests/cases/compiler/genericRecursiveImplicitConstructorErrors3.ts`.
`tsc` reports `TS2314` and `TS2339`, while `tsz` additionally reports `TS2532`.
Root cause: bare generic class/interface self references skipped the TS2314 error-type path during symbol resolution, leaving concrete generic shapes in value flow where tsc uses any-like errorType.
This PR makes those erroneous annotations any-like after TS2314 so property access and inferred return types do not cascade into extra diagnostics.

## Files Touched

- `docs/plan/claims/fix-conformance-quick-pick-20260429-6.md`
- `crates/tsz-checker/src/state/type_resolution/reference_helpers.rs`
- `crates/tsz-checker/tests/ts2314_in_type_literal_suppresses_ts2322_tests.rs`

## Verification

- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run --package tsz-checker --test ts2314_in_type_literal_suppresses_ts2322_tests` (5 passed)
- `cargo nextest run --package tsz-checker --lib` (3007 passed, 10 skipped)
- `./scripts/conformance/conformance.sh run --filter "genericRecursiveImplicitConstructorErrors3" --verbose` (1/1 passed)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL` (`FINAL RESULTS: 12249/12582 passed (97.4%)`)
