# fix(checker): align direct exact-optional undefined write fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/checker-exact-optional-direct-undefined-fingerprints`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

`quick-pick.sh` selected `TypeScript/tests/cases/compiler/strictOptionalProperties1.ts`, a fingerprint-only failure. This slice targets the direct exact-optional property writes where `obj.a = undefined` currently reports TS2322 while tsc reports TS2412 with the exact-optional wording. Tuple-hole display and control-flow anchor mismatches in the same fixture are separate follow-ups.

## Files Touched

- TBD

## Verification

- `cargo nextest run` for the owning crate tests added with the fix.
- `./scripts/conformance/conformance.sh run --filter "strictOptionalProperties1" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
