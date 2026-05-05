# [WIP] fix(checker): align instantiation expression no-crash fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-instantiation-expression-nocrash-fingerprint`
- **PR**: #3245
- **Status**: claim
- **Workstream**: 1 (Conformance / diagnostic fingerprints)

## Intent

The canonical conformance picker selected
`TypeScript/tests/cases/compiler/instantiationExpressionErrorNoCrash.ts`,
a fingerprint-only failure where tsz and tsc agree on diagnostic codes
`TS2344` and `TS2635` but differ in diagnostic fingerprint details.

This PR will root-cause the remaining instantiation-expression no-crash
fingerprint mismatch, add owning Rust regression coverage, and rerun the
targeted conformance test.

## Files Touched

- `crates/tsz-checker/src/state/type_environment/formatting.rs`
- `crates/tsz-checker/src/tests/dispatch_tests.rs`
- `docs/plan/claims/fix-checker-instantiation-expression-nocrash-fingerprint.md`

## Verification

- Baseline target command:
  `./scripts/conformance/conformance.sh run --filter "instantiationExpressionErrorNoCrash" --verbose`
- `CARGO_BUILD_JOBS=1 cargo nextest run -p tsz-checker --lib ts2635`
  - 2 tests passed.
- Planned: targeted conformance rerun for `instantiationExpressionErrorNoCrash`.
  - Attempted twice, but the local `dist-fast` build was interrupted by
    repeated workspace disk exhaustion and one external SIGTERM before the
    filtered case could run.
