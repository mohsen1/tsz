# fix(checker): align intersection weak source fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/intersection-weak-source-fingerprint-20260505`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance fingerprint parity

## Intent

Fix the fingerprint-only divergence in
`TypeScript/tests/cases/conformance/types/intersection/intersectionAsWeakTypeSource.ts`.
The target already emits the expected diagnostic codes (`TS2559`, `TS2739`), so
this PR will root-cause the message, type-display, count, or position mismatch
and fix it in the owning layer rather than suppressing diagnostics.

This fresh claim supersedes the stale unbacked
`docs/plan/claims/claude-exciting-keller-raRPb.md` entry for the same
conformance file, which is marked ready but has no live remote branch or PR.

## Files Touched

- TBD after verbose fingerprint analysis.

## Verification

- `./scripts/conformance/conformance.sh run --filter "intersectionAsWeakTypeSource" --verbose`
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- Focused `cargo nextest run` for the owning-crate regression.
