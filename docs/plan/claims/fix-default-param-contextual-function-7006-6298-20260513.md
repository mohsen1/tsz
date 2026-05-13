# fix(checker): contextually type default parameter functions

- **Date**: 2026-05-13
- **Branch**: `fix-default-param-contextual-function-7006-6298-20260513`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance / contextual typing)

## Intent

Issue #6298 reports a TS7006 false positive for function expressions used as default parameter initializers when the parameter has a function type annotation. The fix should route the parameter annotation as contextual type for the initializer without broadening unrelated callback inference.

## Files Touched

- TBD

## Verification

- TBD
