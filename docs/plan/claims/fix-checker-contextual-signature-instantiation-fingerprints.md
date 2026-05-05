# fix(checker): align contextual signature instantiation fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-contextual-signature-instantiation-fingerprints`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the current fingerprint-only divergence in
`TypeScript/tests/cases/conformance/types/typeRelationships/typeInference/contextualSignatureInstantiation.ts`.
The picker reports matching diagnostic codes (`TS2345`, `TS2403`), so this PR
will root-cause the remaining diagnostic message, span, count, or ordering
mismatch without colliding with the stale merged claim from PR #1929.

## Files Touched

- `docs/plan/claims/fix-checker-contextual-signature-instantiation-fingerprints.md`
  (claim)

## Verification

- owning-crate regression test
- `./scripts/conformance/conformance.sh run --filter "contextualSignatureInstantiation" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/githooks/pre-commit`
