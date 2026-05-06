# fix(checker): align in-keyword typeguard diagnostics

- **Date**: 2026-05-06
- **Branch**: `fix/checker-in-keyword-typeguard-diagnostics`
- **PR**: https://github.com/mohsen1/tsz/pull/3618
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

This PR targets the quick-picked wrong-code conformance mismatch in
`TypeScript/tests/cases/compiler/inKeywordTypeguard.ts`.
TypeScript reports `TS2638` for one case, while tsz currently reports an
extra `TS7053` instead.

## Context

Selected with `scripts/session/quick-pick.sh --seed 3612`.

## Files Touched

- TBD

## Verification

- Pending
