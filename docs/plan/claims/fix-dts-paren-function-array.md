# [WIP] fix(emitter): preserve class returns in inferred function arrays

- **Date**: 2026-05-02
- **Branch**: `fix/dts-paren-function-array`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 2 (declaration emit pass rate)

## Intent

Fix the declaration emit mismatch in `declFileTypeAnnotationParenType` where
an inferred array of arrow functions returning a private class is emitted as
`(() => any)[]` instead of preserving the nameable local class return type.
The slice should stay in AST/type inference and avoid broad printed-string
post-processing.

## Files Touched

- TBD after focused implementation.

## Verification

- Focused emit repro for `declFileTypeAnnotationParenType`.
