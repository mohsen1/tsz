# [WIP] fix(tuple): align restTupleElements1 diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/rest-tuple-elements1-diagnostics`
- **PR**: https://github.com/mohsen1/tsz/pull/3358
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the quick-pick conformance target
`TypeScript/tests/cases/conformance/types/tuple/restTupleElements1.ts`.
The current result has matching broad tuple-related diagnostics but is
wrong-code overall: `TS17019` is missing and an extra `TS2322` is present.

## Files Touched

- `docs/plan/claims/fix-rest-tuple-elements1-diagnostics.md`

## Verification Plan

- Reproduce the target on current `origin/main` with verbose fingerprints
- Focused Rust regression test in the owning crate
- `./target-codex/dist-fast/tsz-conformance --filter 'conformance/types/tuple/restTupleElements1.ts'`
- `./target-codex/dist-fast/tsz-conformance --max 200`
- `scripts/githooks/pre-commit`
