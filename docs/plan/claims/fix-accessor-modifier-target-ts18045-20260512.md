# fix(checker): emit TS18045 for accessor below ES2015

- **Date**: 2026-05-12
- **Branch**: `fix/accessor-modifier-target-ts18045-20260512`
- **Issue**: #5784
- **Status**: claim
- **Workstream**: conformance

## Intent

Verify and, if still needed, fix the missing TS18045 diagnostic for class `accessor` members when the compiler target is below ES2015.

## Scope

- Reproduce the focused issue/conformance case.
- Add the smallest semantic check in the existing class/member validation path if missing.
- Keep the change limited to target-version diagnostics for `accessor` members.

## Verification Plan

- Focused checker/conformance test for TS18045 accessor target gating.
- `cargo fmt --all`
- Relevant direct checker tests or pre-commit before marking ready.
