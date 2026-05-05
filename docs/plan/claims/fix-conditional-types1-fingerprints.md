# [WIP] fix(checker): align conditionalTypes1 fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/conditional-types1-fingerprints`
- **PR**: https://github.com/mohsen1/tsz/pull/3357
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the quick-pick conformance target
`TypeScript/tests/cases/conformance/types/conditional/conditionalTypes1.ts`,
currently categorized as fingerprint-only with matching `TS2322`, `TS2339`,
`TS2403`, `TS2540`, and `TS2542` code sets.

## Files Touched

- `docs/plan/claims/fix-conditional-types1-fingerprints.md`

## Verification Plan

- Reproduce the target on current `origin/main` with verbose fingerprints
- Focused Rust regression test in the owning crate
- `./target-codex/dist-fast/tsz-conformance --filter 'conformance/types/conditional/conditionalTypes1.ts'`
- `./target-codex/dist-fast/tsz-conformance --max 200`
- `scripts/githooks/pre-commit`
