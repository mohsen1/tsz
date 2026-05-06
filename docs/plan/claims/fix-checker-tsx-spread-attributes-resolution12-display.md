# fix(checker): align TSX spread attributes resolution12 display

- **Date**: 2026-05-06
- **Branch**: `fix/checker-tsx-spread-attributes-resolution12-display`
- **PR**: https://github.com/mohsen1/tsz/pull/3518
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

This PR targets the quick-picked fingerprint-only conformance mismatch in
`TypeScript/tests/cases/conformance/jsx/tsxSpreadAttributesResolution12.tsx`.
The diagnostic code set already matches TypeScript (`TS2322`), but tsz reports
extra per-attribute object displays and misses the merged spread-source display.

## Context

PR #1947 already suppressed one any-spread-related extra diagnostic for this
fixture and left follow-up work on merged spread-source display and anchoring.
This claim narrows the remaining work to the quick-picked `tsxSpreadAttributesResolution12`
fingerprint mismatch selected with `scripts/session/quick-pick.sh --seed 3408`.

## Files Touched

- TBD

## Verification

- Pending
