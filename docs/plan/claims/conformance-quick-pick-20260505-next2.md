# fix(checker): align generic construct signature optional parameter diagnostic fingerprint

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-20260505-next2`
- **PR**: TBD
- **Status**: claimed
- **Workstream**: 1 (Conformance fixes)

## Intent

This PR targets the fingerprint-only failure in
`TypeScript/tests/cases/conformance/types/typeRelationships/subtypesAndSuperTypes/subtypingWithGenericConstructSignaturesWithOptionalParameters.ts`.
Both `tsc` and `tsz` emit `TS2430`; the remaining gap is the exact diagnostic
fingerprint.

## Files Touched

- TBD after root-cause analysis.

## Verification

- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo build --profile dist-fast --bin tsz`
- Focused Rust unit tests for the owning crate
- `./scripts/conformance/conformance.sh run --filter "subtypingWithGenericConstructSignaturesWithOptionalParameters" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL`
