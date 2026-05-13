# Preserve literal-union return inference from default parameters

- **Date**: 2026-05-13
- **Branch**: `fix/literal-union-default-param-return-20260513`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance

## Intent

Address #6127, where a function returning a parameter declared as a string literal union with a default initializer is inferred as `string` instead of the declared literal union. The fix should preserve the parameter's declared receiver type through return inference without broadly disabling ordinary literal widening.

## Files Touched

- TBD after investigation.

## Verification

- Pending.
