# fix(checker): align conditionalTypes1 diagnostic display

- **Date**: 2026-05-12
- **Branch**: `fix/conditional-types1-display-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance

## Intent

Close the fingerprint-only `conditionalTypes1.ts` conformance failure. The current
diagnostic code set already matches TypeScript, but several messages expand
conditional or mapped helper aliases too eagerly instead of preserving the source
alias display selected by TypeScript.

## Files Touched

- `docs/plan/claims/fix-conditional-types1-display-20260512.md`
- Checker display/relation files TBD after focused investigation.

## Verification

- Baseline: `.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --filter conditionalTypes1 --print-fingerprints --verbose` (0/1 passed; fingerprint-only; codes match)
