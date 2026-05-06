# fix(checker): accept spread object literals for index signatures

- **Date**: 2026-05-06
- **Branch**: `codex/spread-object-index-signature-20260506`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance)

## Intent

Fix the current `TypeScript/tests/cases/compiler/spreadOfObjectLiteralAssignableToIndexSignature.ts`
conformance failure. The current conformance detail reports one extra `TS2322`
where tsc accepts spreading an object literal into a target with an index signature.

The expected impact is a one-test conformance pass-rate increase while preserving
real index-signature incompatibility diagnostics for non-spread object literals.

## Files Touched

- `docs/plan/claims/codex-spread-object-index-signature-20260506.md`

## Verification

- Pending implementation.
