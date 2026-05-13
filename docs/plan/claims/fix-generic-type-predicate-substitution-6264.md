# fix(checker): substitute generic type predicates during narrowing

- **Date**: 2026-05-13
- **Branch**: `fix-generic-type-predicate-substitution-6264`
- **PR**: TBD
- **Status**: claim
- **Workstream**: issue #6264 type-inference false positive

## Intent

Fix #6264, where calls to generic type-predicate functions with explicit type arguments narrow to unsubstituted predicate types such as `T[]` instead of concrete types like `number[]`. The intended slice is to instantiate predicate return types at call-resolution/narrowing boundaries without changing unrelated control-flow semantics.

## Files Touched

- TBD after implementation

## Verification

- TBD
