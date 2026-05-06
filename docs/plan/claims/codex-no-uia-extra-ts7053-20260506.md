# fix(checker): suppress extra noUncheckedIndexedAccess TS7053

- **Date**: 2026-05-06
- **Branch**: `codex/no-uia-extra-ts7053-20260506`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance)

## Intent

Fix the current `TypeScript/tests/cases/conformance/pedantic/noUncheckedIndexedAccess.ts`
conformance regression where tsz emits one extra `TS7053` while matching the
expected `TS2322` and `TS2344` code set otherwise.

The expected impact is a one-test conformance pass-rate increase by removing a
false-positive indexed-access diagnostic without weakening real implicit-any
indexing diagnostics in adjacent tests.

## Files Touched

- `docs/plan/claims/codex-no-uia-extra-ts7053-20260506.md`

## Verification

- Pending implementation.
