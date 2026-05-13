# fix(parser): reject readonly accessor modifier combination

- **Date**: 2026-05-13
- **Branch**: `fix-readonly-accessor-ts1243-20260513`
- **PR**: TBD
- **Status**: claim
- **Workstream**: Diagnostic conformance

## Intent

Close #6188 by emitting TS1243 when a class auto-accessor combines `readonly` with `accessor`, matching TypeScript's syntax-level modifier compatibility rule. Keep the fix in the syntax/diagnostic layer rather than adding checker semantics, because the invalid modifier pair is determined directly from class member modifiers.

## Files Touched

- `docs/plan/claims/fix-readonly-accessor-ts1243-20260513.md`
- Parser/checker diagnostic files TBD after inspection
- Focused regression test TBD after inspection

## Verification

- Pending
