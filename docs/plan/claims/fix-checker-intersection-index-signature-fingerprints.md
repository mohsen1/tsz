# fix(checker): align intersection index signature fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-intersection-index-signature-fingerprints`
- **PR**: #3261
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only mismatch in
`TypeScript/tests/cases/conformance/types/intersection/intersectionWithIndexSignatures.ts`.
The diagnostic codes and spans already match tsc, but tsz reports aliased
types (`A`, `A & B`) where tsc expands the index-signature value types
(`{ a: string }`, `{ a: string; b: string }`) for the #32484 repro.

## Files Touched

- `docs/plan/claims/fix-checker-intersection-index-signature-fingerprints.md`

## Verification

- `./scripts/conformance/conformance.sh run --filter "intersectionWithIndexSignatures" --verbose`
- focused checker/solver regression test once the responsible formatter or type relation path is identified
- `./scripts/conformance/conformance.sh run --max 200`
- `PATH="$HOME/.cargo/bin:$PATH" scripts/githooks/pre-commit`
