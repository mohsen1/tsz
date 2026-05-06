# fix(checker): suppress false TS2551 for JSDoc class method declarations

- **Date**: 2026-05-06
- **Branch**: `fix/checker-jsdoc-class-method-declarations-ts2551`
- **PR**: #3698
- **Status**: abandoned
- **Workstream**: 1 (Diagnostic conformance)

## Intent

`quick-pick.sh` selected `TypeScript/tests/cases/conformance/jsdoc/declarations/jsDeclarationsClassMethod.ts`, a false-positive failure where tsz emits TS2551 and tsc emits no diagnostics. This PR will root-cause the extra property-name diagnostic around JSDoc class/prototype/static method declaration patterns and suppress or reroute it at the owning semantic layer without masking real property errors.

Abandoned before implementation: after updating to `origin/main`, the targeted verbose conformance run passes 1/1, so this claim no longer represents an active failure.

## Files Touched

- `docs/plan/claims/fix-checker-jsdoc-class-method-declarations-ts2551.md`

## Verification

- `cargo nextest run` for the owning crate tests added with the fix.
- `./scripts/conformance/conformance.sh run --filter "jsDeclarationsClassMethod" --verbose` (passes on current `origin/main`; no implementation needed)
