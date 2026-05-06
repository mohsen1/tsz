# fix(checker): align mixin access modifier diagnostics

- **Date**: 2026-05-06
- **Branch**: `fix/checker-mixin-access-modifiers-fingerprints`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

`quick-pick.sh --run` selected `TypeScript/tests/cases/conformance/classes/mixinAccessModifiers.ts`, an XFAIL with matching diagnostic codes but fingerprint drift around private/protected property access on intersections and classes derived from mixin constructor intersections. This PR will focus on the access-modifier diagnostic path so tsz reports the same private/protected errors and receiver names as tsc instead of falling back to missing-property diagnostics where the member exists but is inaccessible.

## Files Touched

- TBD

## Verification

- `cargo nextest run` for the owning crate tests added with the fix.
- `./scripts/conformance/conformance.sh run --filter "mixinAccessModifiers" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
