# fix(checker): align variadicTuples1 fingerprints

- **Date**: 2026-05-12
- **Branch**: `fix/variadic-tuples1-fingerprints-20260512`
- **Base**: `fix/conditional-types1-fingerprints-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance

## Intent

Reduce the remaining non-XFAIL conformance fingerprint-only failures after the current `conditionalTypes1` slice. This slice targets `variadicTuples1.ts`, which is the other remaining non-XFAIL mismatch in the local full conformance snapshot.

## Files Touched

- `docs/plan/claims/fix-variadic-tuples1-fingerprints-20260512.md`

## Verification

- Baseline: full local conformance snapshot after the index-signatures slice reported only `conditionalTypes1` and `variadicTuples1` as non-XFAIL fingerprint-only failures.
