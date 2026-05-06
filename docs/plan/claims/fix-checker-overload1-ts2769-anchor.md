# fix(checker): align overload1 TS2769 anchor

- **Date**: 2026-05-06
- **Branch**: `fix/checker-overload1-ts2769-anchor`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Reduce the fingerprint-only conformance failure in
`TypeScript/tests/cases/compiler/overload1.ts`. Both tsc and tsz emit
`TS2322`, `TS2554`, and `TS2769`, but the ambiguous overload call
`z=x.g(new O.B())` anchors `TS2769` differently. tsc reports the diagnostic
at the assignment target (`z`, column 5), while tsz currently reports it at
the call expression (`x`, column 7).

## Files Touched

- `docs/plan/claims/fix-checker-overload1-ts2769-anchor.md`

## Verification

- Pending.
