# fix(checker): align enumLiteralTypes3 fingerprints

- **Date**: 2026-05-12
- **Branch**: `fix/enum-literal-types3-fingerprints-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Close the fingerprint-only conformance failures for:

- `TypeScript/tests/cases/conformance/types/enum/enumLiteralTypes3.ts`
- `TypeScript/tests/cases/conformance/types/enum/stringEnumLiteralTypes3.ts`

Both representatives report matching diagnostic codes with TS2322/TS2367/TS2678
fingerprint drift, likely in enum literal diagnostic display or anchoring.

## Files Touched

- TBD after investigation.

## Verification

- Baseline: `scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter 'enumLiteralTypes3|stringEnumLiteralTypes3' --verbose`
- Planned: focused checker regression covering the shared enum literal display/anchor drift
- Planned: `scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter 'enumLiteralTypes3|stringEnumLiteralTypes3' --verbose`
