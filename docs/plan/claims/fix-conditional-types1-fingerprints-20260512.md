# fix(checker): align conditionalTypes1 fingerprints

- **Date**: 2026-05-12
- **Branch**: `fix/conditional-types1-fingerprints-20260512`
- **Base**: `fix/index-signatures1-fingerprint-clean-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance

## Intent

Reduce the remaining conformance fingerprint-only failures after the current index/variance/recursive slices. This slice targets `conditionalTypes1.ts`, which currently has eight missing and twelve extra fingerprints in the local full conformance snapshot.

## Files Touched

- `docs/plan/claims/fix-conditional-types1-fingerprints-20260512.md`

## Verification

- Baseline: full local conformance snapshot reports only `conditionalTypes1` and `variadicTuples1` as non-XFAIL fingerprint-only failures.
