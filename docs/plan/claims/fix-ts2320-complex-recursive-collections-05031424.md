# [WIP] fix(checker): report conflicting recursive interface bases

- **Date**: 2026-05-03
- **Branch**: `fix/ts2320-complex-recursive-collections-05031424`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Investigate and fix `complexRecursiveCollections.ts`, where tsc reports
TS2320 for interfaces that simultaneously extend recursive collection bases
while tsz reports lower-level TS2430 inheritance diagnostics instead. The fix
should preserve the underlying assignability checks but choose the tsc-parity
diagnostic when multiple inherited base interfaces conflict at the same
declaration.

## Files Touched

- TBD

## Verification

- `scripts/session/quick-pick.sh --run` (selected and reproduced
  `complexRecursiveCollections.ts`; missing TS2320 with extra TS2430
  fingerprints).
