# fix(emit): lower static-field class expressions in parameter initializers

- **Date**: 2026-05-12
- **Branch**: `fix/emit-static-field-param-initializer-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 2 (JS emit pass rate)

## Intent

Fix the live JavaScript emit mismatch for
`classWithStaticFieldInParameterInitializer`. ESNext already matches TypeScript,
but ES2015/ES5 lowering emits the class expression directly inside the parameter
initializer instead of matching TypeScript's transformed default-parameter shape
and class-name preservation around the static field assignment.

## Files Touched

- `docs/plan/claims/fix-emit-static-field-param-initializer-20260512.md`

## Verification

- Pending
