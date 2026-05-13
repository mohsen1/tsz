# fix(checker): report TS18045 for accessor below ES2015

- **Date**: 2026-05-13
- **Branch**: `fix-accessor-modifier-es5-18045-6291-20260513`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance / target feature checks)

## Intent

Issue #6291 reports that `accessor` class properties are accepted when targeting ES5, while tsc emits TS18045. The fix should use existing class-member declaration validation and add a focused CLI/checker regression without changing ES2015+ behavior.

## Files Touched

- TBD

## Verification

- TBD
