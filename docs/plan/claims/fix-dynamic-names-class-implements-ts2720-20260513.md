# Fix dynamic-name class implements TS2720 false positive

- **Date**: 2026-05-13
- **Branch**: `fix/dynamic-names-class-implements-ts2720-20260513`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance

## Intent

Fix the remaining `dynamicNames` conformance regression where a class with public computed-name members structurally implements another public computed-name class but tsz emits TS2720. The change should preserve whole-type assignability side effects for declaration emit while suppressing only the erroneous class-level diagnostic after member-level compatibility succeeds.

## Files Touched

- `crates/tsz-checker/src/classes/class_implements_checker/core.rs`
- `crates/tsz-checker/tests/class_implements_predicate_inference_tests.rs`

## Verification

- Pending.
