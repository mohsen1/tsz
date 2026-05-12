# fix-union-function-contextual-implicit-any-ts7006-20260512

Status: claim
Owner: Codex
Branch: fix-union-function-contextual-implicit-any-ts7006-20260512
Issue: #5840

## Scope

Match TypeScript contextual typing for arrow/function expressions assigned to a union of incompatible function types: when parameter contextual types conflict, leave the parameter implicit-any so TS7006 fires under noImplicitAny/strict.

## Plan

- Add a focused TS7006 regression for `((x: string) => void) | ((x: number) => void)`.
- Trace contextual signature synthesis for union call signatures.
- Disable synthetic union contextual parameter types when candidate parameter types are incompatible for tsc-style contextual typing.
