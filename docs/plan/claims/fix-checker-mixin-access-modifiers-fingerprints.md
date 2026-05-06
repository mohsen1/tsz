# fix(checker): align mixin access modifier diagnostics

- **Date**: 2026-05-06
- **Branch**: `fix/checker-mixin-access-modifiers-fingerprints`
- **PR**: #3704
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

`quick-pick.sh --run` selected `TypeScript/tests/cases/conformance/classes/mixinAccessModifiers.ts`, an XFAIL with matching diagnostic codes but fingerprint drift around private/protected property access on intersections and classes derived from mixin constructor intersections. This PR focuses on the direct intersection access path: private intersections now report the same `never` missing-property diagnostics as tsc, all-protected intersections report the intersection owner, and protected/public intersections remain publicly accessible. The remaining generic application and mixin-derived class drift is intentionally left for follow-up PRs.

## Files Touched

- `crates/tsz-checker/src/checkers/property_checker.rs`
- `crates/tsz-solver/src/intern/intersection.rs`
- `crates/tsz-solver/src/intern/normalize.rs`
- `crates/tsz-solver/src/objects/collect.rs`

## Verification

- `cargo nextest run` for the owning crate tests added with the fix.
- `./scripts/conformance/conformance.sh run --filter "mixinAccessModifiers" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
