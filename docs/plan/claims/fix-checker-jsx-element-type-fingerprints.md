# fix(checker): align jsxElementType diagnostic fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-jsx-element-type-fingerprints`
- **PR**: #3200
- **Status**: ready
- **Workstream**: 1 (Conformance - JSX diagnostic fingerprints)

## Intent

The canonical conformance picker selected
`TypeScript/tests/cases/compiler/jsxElementType.tsx`, currently a
fingerprint-only failure: `tsz` emits the same diagnostic code set as `tsc`
(`TS2304`, `TS2322`, `TS2339`, `TS2741`, `TS2769`, `TS2786`) but differs in
one or more diagnostic fingerprints.

This PR will inspect the remaining message/anchor/display divergence, fix the
root cause in the appropriate checker/solver/printer layer, and add a focused
Rust regression test for the invariant.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/orchestration/resolution.rs`
- `crates/tsz-checker/src/checkers/jsx/orchestration/component_props.rs`
- `crates/tsz-checker/src/checkers/jsx/extraction.rs`
- `crates/tsz-checker/src/checkers/jsx/props/resolution.rs`
- `crates/tsz-checker/src/checkers/jsx/tests.rs`

## Verification

- `cargo check --package tsz-checker`
- `cargo nextest run -p tsz-checker --lib` -> 3426 passed, 10 skipped
- `cargo nextest run -p tsz-checker --test conformance_issues test_module_local_jsx_namespace_does_not_satisfy_global_jsx_lookup` -> 1 passed
- `./scripts/conformance/conformance.sh run --filter "jsxElementType" --verbose` -> 3/3 passed
- `./scripts/conformance/conformance.sh run --filter "jsxPropsAsIdentifierNames" --verbose` -> 1/1 passed
- `./scripts/conformance/conformance.sh run --max 200` -> 200/200 passed
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL` -> `FINAL RESULTS: 12456/12582 passed (99.0%)`
- `./scripts/conformance/conformance.sh diff || true` -> net `12451 -> 12456 (+5)`; remaining PASS -> FAIL entries are non-JSX `.ts` tests outside this branch's touched code.
