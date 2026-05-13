# fix(checker): reject InstanceType on private constructors

- **Date**: 2026-05-13
- **Branch**: `fix-private-constructor-instancetype-6194-20260513`
- **PR**: TBD
- **Status**: claim
- **Workstream**: Diagnostic conformance

## Intent

Close #6194 by making constructor accessibility participate in generic constraint validation for utility types such as `InstanceType<T extends abstract new (...args: any) => any>`. A class value with a private or protected constructor must not satisfy a public/abstract construct-signature constraint, even though it has a construct signature structurally.

## Files Touched

- `docs/plan/claims/fix-private-constructor-instancetype-6194-20260513.md`
- Checker constructor/generic constraint code TBD after implementation
- Focused TS2344 regression test TBD after implementation

## Verification

- Pending
