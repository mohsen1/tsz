# [WIP] fix(checker): align exact optional inference TS2345 fingerprint

- **Date**: 2026-04-29
- **Branch**: `fix/checker-exact-optional-inference-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Claim the random conformance target `TypeScript/tests/cases/compiler/inferenceExactOptionalProperties2.ts`.
The current snapshot reports a fingerprint-only TS2345 mismatch: TypeScript and
TSZ agree on the code, but TSZ's diagnostic fingerprint diverges. This PR will
identify the display, anchor, or elaboration invariant behind that mismatch and
fix it in the owning checker/solver boundary with a focused regression test.

## Files Touched

- `docs/plan/claims/fix-checker-exact-optional-inference-fingerprint.md`
- Implementation files TBD after verbose conformance diagnosis.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "inferenceExactOptionalProperties2" --verbose`
- Planned: owning-crate `cargo nextest run` target for the regression test.
