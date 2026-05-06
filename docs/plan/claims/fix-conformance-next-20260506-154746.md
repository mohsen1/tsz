# fix(checker): align long object instantiation property fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-next-20260506-154746`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Target the quick-pick conformance failure:
`TypeScript/tests/cases/compiler/longObjectInstantiationChain2.ts`.

`tsc` and tsz already agree on diagnostic code `TS2339`, but the
fingerprints differ. This slice will diagnose whether the drift is property
access anchoring, instantiated object display, or deep mapped/intersection
chain evaluation, then align the fingerprints without changing the intended
diagnostic set.

## Files Touched

- TBD

## Verification

- TBD
