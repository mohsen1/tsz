# fix(checker): align function call arity fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-next-20260506-113731`
- **PR**: #4023
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Target the quick-pick conformance failure:
`TypeScript/tests/cases/compiler/functionCall11.ts`.

`tsc` and tsz already agree on the diagnostic codes (`TS2345`, `TS2554`) for
this function-call fixture, but the conformance fingerprints differ. This
slice will identify whether the mismatch is diagnostic anchoring, message
formatting, or arity reporting, then align the fingerprints while preserving
the existing code set.

## Files Touched

- `crates/tsz-checker/src/types/computation/call_result.rs`
- `crates/tsz-checker/tests/call_resolution_regression_tests.rs`
- `docs/plan/claims/fix-conformance-next-20260506-113731.md`

## Verification

- `cargo fmt --check`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/conformance-next-20260506-113731 CARGO_BUILD_JOBS=2 cargo check -p tsz-checker --lib`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/conformance-next-20260506-113731 CARGO_BUILD_JOBS=2 cargo nextest run -p tsz-checker --test call_resolution_regression_tests -E 'test(argument_mismatch_display_uses_declared_parameter_not_sibling_literal)'`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/conformance-next-20260506-113731 CARGO_BUILD_JOBS=2 cargo nextest run -p tsz-checker --test call_resolution_regression_tests --test generic_call_inference_tests`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/conformance-next-20260506-113731 CARGO_BUILD_JOBS=2 cargo nextest run -p tsz-checker --lib`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/conformance-next-20260506-113731 CARGO_BUILD_JOBS=2 cargo nextest run -p tsz-solver --lib`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/conformance-next-20260506-113731 CARGO_BUILD_JOBS=2 ./scripts/conformance/conformance.sh run --filter "functionCall11" --verbose --test-dir /Users/mohsen/code/tsz/.worktrees/fix-large-ts-repo-signature-param-reserve-20260506/TypeScript/tests/cases`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/conformance-next-20260506-113731 CARGO_BUILD_JOBS=2 ./scripts/conformance/conformance.sh run --max 200 --test-dir /Users/mohsen/code/tsz/.worktrees/fix-large-ts-repo-signature-param-reserve-20260506/TypeScript/tests/cases`

The `--max 200` smoke run reported one existing fingerprint-only failure in
`anyIndexedAccessArrayNoException.ts`. A detached worktree at clean
`origin/main` reproduced the same `TS2538` one-column drift, so it is not
introduced by this slice.
