# Preserve literal inference through generic identity calls

- **Date**: 2026-05-13
- **Branch**: `fix/generic-identity-preserve-literal-20260513`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance

## Intent

Address #6126, where a generic identity call such as `identity("test")` widens the inferred `T` to `string`, causing a false TS2322 when assigning the result to `"test"`.

## Files Touched

- TBD after investigation.

## Verification

- Pending.
