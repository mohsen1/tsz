# [WIP] fix(checker): align interface intersection extends fingerprints

- **Date**: 2026-04-29
- **Branch**: `fix/interface-extends-intersection-fingerprint`
- **PR**: #1717
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only mismatch picked by `scripts/session/quick-pick.sh`
for `interfaceExtendsObjectIntersectionErrors.ts`. The conformance snapshot
shows matching diagnostic codes (`TS2312`, `TS2411`, `TS2413`, `TS2416`,
`TS2430`) but differing fingerprints, so this slice will diagnose whether the
gap is message rendering, type display, or diagnostic anchoring and fix it in
the owning checker/solver boundary.

## Files Touched

- `docs/plan/claims/fix-interface-extends-intersection-fingerprint.md`
- Implementation files TBD after diagnosis.

## Verification

- `cargo check --package tsz-checker` (passes)
- `cargo nextest run --package tsz-checker --test interface_heritage_display_tests` (2 tests pass)
- `./scripts/conformance/conformance.sh run --filter "interfaceExtendsObjectIntersectionErrors" --verbose` (still fingerprint-only; this WIP removes the alias-name extra fingerprints, but remaining missing diagnostics are tuple/class/index-signature/union-heritage emission gaps)
