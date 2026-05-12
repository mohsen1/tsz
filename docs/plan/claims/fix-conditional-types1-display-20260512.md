# fix(checker): align conditionalTypes1 diagnostic display

- **Date**: 2026-05-12
- **Branch**: `fix/conditional-types1-display-20260512`
- **PR**: TBD
- **Status**: abandoned
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
- Attempted solver/checker display-alias provenance changes around generic conditional/indexed aliases; rebuilt with `cargo build --profile dist-fast -p tsz-cli -p tsz-conformance` and reran the same focused conformance command. Result was unchanged, so speculative code changes were reverted and this claim was abandoned for a future deeper investigation.
