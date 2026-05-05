# fix(checker): collapse optional-property intersection display

- **Date**: 2026-05-05
- **Branch**: `fix-intersection-optional-display-20260505`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance fingerprint parity

## Intent

Fix the fingerprint-only TS2322 divergence in
`TypeScript/tests/cases/compiler/intersectionsAndOptionalProperties.ts`.
The existing hand-off claim
`investigate-diagnostic-type-display-alias-preservation.md` records this as an
intersection display issue: `tsc` collapses `{ a: null; } & { b: string; }`
to `{ a: null; b: string; }` in the diagnostic surface, while `tsz` preserves
the split intersection form.

## Files Touched

- TBD after verbose fingerprint analysis.

## Verification

- `./scripts/conformance/conformance.sh run --filter "intersectionsAndOptionalProperties" --verbose`
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- Focused `cargo nextest run` for the owning-crate regression.
