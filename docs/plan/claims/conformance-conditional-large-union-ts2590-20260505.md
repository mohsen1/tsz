# [WIP] fix(conformance): avoid false TS2590 on conditional large-union access

- **Date**: 2026-05-05
- **Branch**: `conformance/conditional-large-union-ts2590-20260505`
- **PR**: #3208
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

The random conformance picker selected
`TypeScript/tests/cases/compiler/conditionalTypeDiscriminatingLargeUnionRegularTypeFetchingSpeedReasonable.ts`.
`tsc` accepts this performance-oriented conditional-type case, but `tsz` emits
an extra TS2590 ("Expression produces a union type that is too complex to
represent."). This PR will identify the root cause of the false complexity
diagnostic and fix it in the owning semantic layer rather than suppressing the
diagnostic at the conformance boundary.

## Files Touched

- `crates/tsz-solver/src/intern/core/constructors.rs` (large object-union complexity flag contract)
- `crates/tsz-solver/tests/intern_tests.rs` (solver regression for representable large object unions)
- `crates/tsz-checker/tests/conformance_issues/errors/runtime.rs` (checker regression for the Record/discriminated-union helper)

## Verification

- `cargo fmt --check`
- `CARGO_BUILD_JOBS=1 cargo check --package tsz-solver`
- `CARGO_BUILD_JOBS=1 cargo check --package tsz-checker`
- `CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 cargo build --target-dir .target --profile dist-fast -p tsz-cli --bin tsz`
- `CARGO_BUILD_JOBS=1 cargo nextest run --package tsz-solver test_large_object_union_preserved_without_too_complex_flag` (1 passed)
- `CARGO_BUILD_JOBS=1 cargo nextest run --package tsz-checker test_no_false_ts2344_for_discriminated_union_record_helper` (1 passed)
- `CARGO_BUILD_JOBS=1 cargo nextest run --package tsz-solver --lib` (5656 passed, 9 skipped)
- `CARGO_BUILD_JOBS=1 cargo nextest run --package tsz-checker --lib` (3449 passed, 10 skipped)
- `CARGO_BUILD_JOBS=1 ./scripts/conformance/conformance.sh run --filter "conditionalTypeDiscriminatingLargeUnionRegularTypeFetchingSpeedReasonable" --verbose` (1/1 passed)
- `CARGO_BUILD_JOBS=1 ./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
- `CARGO_BUILD_JOBS=1 scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL` (`FINAL RESULTS: 11912/12582 passed (94.7%)`; targeted test present as PASS in `scripts/conformance/conformance-last-run.txt`)
