# [WIP] fix(checker): align control-flow function-like diagnostics

- **Date**: 2026-05-03
- **Branch**: `fix/control-flow-function-like-fingerprint-05031915`
- **PR**: #2612
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Investigate and fix the fingerprint-only mismatch in
`TypeScript/tests/cases/compiler/controlFlowForFunctionLike1.ts`.
`tsc` and `tsz` agree on TS2345, but `tsz` anchors or renders an extra
fingerprint for `test.ts:22:12`:

```text
Argument of type 'string' is not assignable to parameter of type 'number'.
```

## Files Touched

- TBD

## Verification

- `scripts/session/quick-pick.sh --run` selected and reproduced
  `TypeScript/tests/cases/compiler/controlFlowForFunctionLike1.ts`
  as a fingerprint-only failure with matching `[TS2345]`.
