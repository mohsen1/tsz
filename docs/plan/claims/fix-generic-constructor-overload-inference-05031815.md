# [WIP] fix(checker): align overloaded constructor inference diagnostics

- **Date**: 2026-05-03
- **Branch**: `fix/generic-constructor-overload-inference-05031815`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Investigate and fix `genericCallWithOverloadedConstructorTypedArguments.ts`,
where `tsc` reports TS2454 and TS2769, while `tsz` additionally emits TS2345
for generic calls whose argument is an overloaded construct-signature object.

## Files Touched

- TBD

## Verification

- `scripts/session/quick-pick.sh --run` selected and reproduced
  `TypeScript/tests/cases/conformance/types/typeRelationships/typeInference/genericCallWithOverloadedConstructorTypedArguments.ts`
  with expected `[TS2454, TS2769]` and actual `[TS2345, TS2454, TS2769]`.
