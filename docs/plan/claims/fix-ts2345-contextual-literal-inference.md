# [WIP] fix(checker): align TS2345 contextual literal inference fingerprint

- **Date**: 2026-05-05
- **Branch**: `fix-ts2345-contextual-literal-inference`
- **PR**: #2762
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the refreshed random conformance pick
`paramsOnlyHaveLiteralTypesWhenAppropriatelyContextualized.ts`, where `tsc`
and `tsz` both emit TS2345 but disagree on the diagnostic fingerprint.
The suspected surface is generic call inference and the display of literal
arguments when a homomorphic mapped type provides lower-priority contextual
inference.

## Files Touched

- `docs/plan/claims/fix-ts2345-contextual-literal-inference.md`
- Production and test files TBD after root-cause diagnosis.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "paramsOnlyHaveLiteralTypesWhenAppropriatelyContextualized" --verbose`.
- Planned: owning-crate unit tests for the fixed invariant.
- Planned: relevant `cargo check`, `cargo nextest run`, and conformance regression checks before marking ready.
