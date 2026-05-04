# [WIP] fix(checker): match merge symbol reexport function diagnostics

- **Date**: 2026-05-04
- **Branch**: `fix/merge-symbol-reexport-function`
- **PR**: #2720
- **Status**: claim
- **Workstream**: 1 (Conformance)

## Intent

Fix the randomly picked conformance failure
`TypeScript/tests/cases/compiler/mergeSymbolRexportFunction.ts`. The expected
tsc fingerprint is TS2451, while tsz currently emits TS1362, TS2300, and
TS2349. This PR will identify the binding/checking root cause and add a focused
Rust regression test for the owning invariant.

## Files Touched

- TBD after diagnosis.

## Verification

- TBD before ready.
