# fix(checker): apply this-predicate property narrowing

- **Date**: 2026-05-13
- **Branch**: `fix-this-predicate-property-narrowing-6299-20260513`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance / control-flow narrowing)

## Intent

Issue #6299 reports that method type predicates of the form `this is C<T> & { value: T }` do not narrow property accesses on the receiver. The fix should apply the predicate's narrowed receiver type through the existing control-flow/type-guard path without special-casing the repro.

## Files Touched

- TBD

## Verification

- TBD
