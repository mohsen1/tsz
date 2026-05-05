# [WIP] fix(checker): align constructor function extension diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-class-constructor-extension-diagnostics`
- **PR**: https://github.com/mohsen1/tsz/pull/3118
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the current wrong-code divergence in
`TypeScript/tests/cases/conformance/salsa/classCanExtendConstructorFunction.ts`.
The picker reports a missing `TS2416` and extra `TS2339`, so this PR will
root-cause the class-extension/constructor-function diagnostic path and align
the emitted diagnostics.

## Files Touched

- `docs/plan/claims/fix-checker-class-constructor-extension-diagnostics.md`
  (claim)

## Verification

- owning-crate regression test
- `./scripts/conformance/conformance.sh run --filter "classCanExtendConstructorFunction" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/githooks/pre-commit`
