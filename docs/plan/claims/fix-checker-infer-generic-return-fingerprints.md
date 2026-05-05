# fix(checker): align generic return inference fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-infer-generic-return-fingerprints`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the remaining fingerprint-only divergence in
`TypeScript/tests/cases/compiler/inferFromGenericFunctionReturnTypes3.ts`.
The previous TS2769 mismatch is merged, but the test still reports matching
diagnostic codes with incorrect spans/messages around literal-preserving
generic return inference and the `bar(() => ... ? [{ state: State.A }] :
[{ state: State.B }])` callback.

## Files Touched

- `docs/plan/claims/fix-checker-infer-generic-return-fingerprints.md`
  (claim)

## Verification

Planned:

- `cargo fmt --all -- --check`
- `cargo nextest run -p tsz-checker --test <focused-test>`
- `./scripts/conformance/conformance.sh run --filter "inferFromGenericFunctionReturnTypes3" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/githooks/pre-commit`
