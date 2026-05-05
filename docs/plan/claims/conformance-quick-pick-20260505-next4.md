# [WIP] fix(checker): preserve tuple literal inference from readonly tuple assertions

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-20260505-next4`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the conformance fingerprint mismatch in `typeInferenceWithTupleType.ts`.
The picked failure has matching diagnostic codes, but tsz emits extra TS2322
fingerprints for `expected = f1(undefined as ["a"[], "b"[]])` and the readonly
tuple overload because tuple inference widens the inferred `T1` to `string`
where `tsc` preserves the literal `"a"`.

## Files Touched

- TBD after root-cause inspection.

## Verification

- `scripts/session/quick-pick.sh --run` selected and reproduced
  `TypeScript/tests/cases/conformance/types/tuple/typeInferenceWithTupleType.ts`.
