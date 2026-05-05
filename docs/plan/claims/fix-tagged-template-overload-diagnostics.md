# fix(checker): preserve tagged-template overload contextual diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/tagged-template-overload-diagnostics`
- **PR**: #2760
- **Status**: ready
- **Workstream**: Conformance / tagged template overload diagnostics

## Intent

Fix the conformance mismatch in:

- `TypeScript/tests/cases/conformance/es6/templates/taggedTemplateStringsWithOverloadResolution3.ts`
- `TypeScript/tests/cases/conformance/es6/templates/taggedTemplateStringsWithOverloadResolution3_ES6.ts`

The current checker keeps the right broad overload error codes but loses
contextual typing for the function substitutions in the final tagged-template
overload pair. That produces extra `TS7006` implicit-any diagnostics where
TypeScript contextually types the parameter, and misses the expected
`TS2551` body diagnostic on `n.toFixed()` against the string overload.

The same run also misses the `TS2722` possibly-undefined invocation on `d2()`,
so the fix will investigate tagged-template overload result typing and
contextual substitution typing together, without hardcoding this fixture.

## Overlap Check

`fix-checker-tagged-template-overload-arity-contextual.md` / PR #1326 fixed
a prior tagged-template overload arity contextual-typing false positive in a
different fixture. This target still fails on current `origin/main`, so this
claim covers the remaining diagnostics behavior.

## Verification

- `./scripts/conformance/conformance.sh run --filter "taggedTemplateStringsWithOverloadResolution3" --verbose` currently fails 0/2 with missing `TS2551`/`TS2722`, extra `TS7006`, and additional `TS2769`/`TS2322` fingerprint drift.
- `./scripts/conformance/conformance.sh run --filter "taggedTemplateStringsWithOverloadResolution3" --verbose` passes 2/2 with no fingerprint-only drift.
- Regression filters pass for `genericTemplateOverloadResolution`, `taggedTemplatesWithTypeArguments1`, `jsDeclarationsTypeAliases`, `typedefTagTypeResolution`, and `importTag17`.
- `cargo nextest run --package tsz-checker --lib ts2769_tagged_template_anchors_after_nullish_recovery jsdoc_broken_typedef_body_recovers_alias_as_any` passes 2/2.
- `cargo nextest run --package tsz-checker --test conformance_issues test_tagged_template` passes 7/7.
- `cargo check --package tsz-checker` passes.
- `./scripts/safe-run.sh ./scripts/conformance/conformance.sh run` passes with no regressions and improves full conformance from `12436 -> 12441` (+5):
  - `TypeScript/tests/cases/conformance/es6/templates/taggedTemplateStringsWithOverloadResolution3.ts`
  - `TypeScript/tests/cases/conformance/es6/templates/taggedTemplateStringsWithOverloadResolution3_ES6.ts`
  - `TypeScript/tests/cases/conformance/salsa/prototypePropertyAssignmentMergeWithInterfaceMethod.ts`
  - `TypeScript/tests/cases/conformance/types/typeRelationships/subtypesAndSuperTypes/subtypingWithGenericConstructSignaturesWithOptionalParameters.ts`
  - `TypeScript/tests/cases/conformance/types/typeRelationships/typeInference/unionAndIntersectionInference1.ts`
