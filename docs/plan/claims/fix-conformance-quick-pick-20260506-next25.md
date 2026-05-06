# [WIP] fix(checker): align deeply nested mapped type fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-quick-pick-20260506-next25`
- **PR**: #3819
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Claiming `TypeScript/tests/cases/compiler/deeplyNestedMappedTypes.ts`.

Current `origin/main` emits `TS2322`, but the fingerprints differ from tsc:

- Missing `TS2322` at `test.ts:18:7` for the nested `Id2<...>` assignment.
- Missing `TS2322` at `test.ts:70:5`, `test.ts:74:5`, and `test.ts:78:5` where `Input[]` should display as its expanded object-array type.
- Extra `TS2322` at `test.ts:74:5` using the alias display `Input[]`.

This slice will align assignment diagnostic source display for deeply nested
mapped/static schema types without changing the TS2322 code surface.

## Files Touched

- TBD after root-cause investigation.

## Verification

- Baseline: `./scripts/conformance/conformance.sh run --filter "deeplyNestedMappedTypes" --verbose` (fingerprint-only failure)
