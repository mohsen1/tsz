# [WIP] fix(checker): align recursive complicated class fingerprints

- **Date**: 2026-04-29
- **Branch**: `fix/conformance-quick-pick-20260429-4`
- **PR**: #1809
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance and fingerprints)

## Intent

Target the random conformance pick `TypeScript/tests/cases/compiler/recursiveComplicatedClasses.ts`.
The current failure is fingerprint-only: TSZ emits the same diagnostic codes as
`tsc` (`TS2300`, `TS2322`, `TS2345`, `TS2454`, `TS2507`, `TS2564`) but differs
in message, count, or anchor fingerprints. This PR will diagnose the shared
root cause and fix it in the appropriate checker/solver/printer boundary.

## Files Touched

- TBD after investigation

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "recursiveComplicatedClasses" --verbose`
- Planned: relevant `cargo nextest run` package filters for touched crates
- Planned: `./scripts/conformance/conformance.sh run --max 200`
