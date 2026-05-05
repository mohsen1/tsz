# fix(checker): align template literal pattern fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-template-literal-patterns-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Follow up on PR #1811's partial `templateLiteralTypesPatterns.ts` improvement
and close the remaining fingerprint drift in the same fixture. The current
random pick shows matching TS2322/TS2345 code families in the snapshot, while a
fresh verbose run on `origin/main` exposes remaining alias-display,
template-number-pattern, generic variance, and duplicate-declaration drift.
This PR will keep the slice scoped to the picked fixture and fix root causes in
solver/query or diagnostic formatting layers rather than adding checker-local
suppression.

## Files Touched

- TBD after diagnosis.

## Verification

- `./scripts/conformance/conformance.sh run --filter "templateLiteralTypesPatterns" --verbose` (baseline captured; currently fails with extra TS2403 plus fingerprint drift)
- Additional targeted unit tests and conformance regression runs will be added before marking ready.
