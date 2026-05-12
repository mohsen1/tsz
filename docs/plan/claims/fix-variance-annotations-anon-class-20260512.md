# fix(checker): suppress varianceAnnotations anonymous-class extras

- **Date**: 2026-05-12
- **Branch**: `fix/variance-annotations-anon-class-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance

## Intent

Continue reducing `varianceAnnotations.ts` fingerprint-only drift after the TS2345 display fix. This slice targets only the two extra TS2322 diagnostics on the anonymous class repro (`InstanceType<Anon<T>>`) and leaves the remaining missing `Baz<string>` diagnostic for a separate semantic slice.

## Files Touched

- `docs/plan/claims/fix-variance-annotations-anon-class-20260512.md`

## Verification

- Pending baseline: focused `varianceAnnotations` conformance run on current main.
