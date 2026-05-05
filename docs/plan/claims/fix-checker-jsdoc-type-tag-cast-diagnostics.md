# fix(checker): align JSDoc type-tag cast diagnostics

- **Branch**: `fix/checker-jsdoc-type-tag-cast-diagnostics`
- **Status**: Ready
- **Workstream**: 1 (Diagnostic conformance)
- **Target**: `TypeScript/tests/cases/conformance/jsdoc/jsdocTypeTagCast.ts`

## Intent

Fix the wrong-code conformance gap in `jsdocTypeTagCast.ts`.

Current divergence:

- Missing `TS1228` for a JSDoc `@type` cast whose type expression is a type
  predicate (`numOrStr is string`).
- Extra `TS2403` for the later `var s` declaration in the same JS file.
- TS2322 display drift where `tsc` reports `SomeFakeClass`, while `tsz`
  currently reports the structural object shape.

## Scope

- Diagnose the root cause in the JSDoc type-tag cast path.
- Add focused checker regression coverage for invalid type-predicate casts and
  any related duplicate-declaration or display invariant needed by the fix.
- Keep the change in checker diagnostics/orchestration unless investigation
  shows a solver boundary issue.

## Verification

- `cargo fmt --all --check` - passed
- `cargo check --package tsz-checker --package tsz-solver` - passed
- `cargo nextest run --package tsz-checker --test jsdoc_type_tag_tests test_jsdoc_type_predicate_cast_emits_ts1228 test_jsdoc_any_cast_string_concat_redeclaration_no_ts2403 test_js_constructor_instance_assignment_source_uses_constructor_name` - passed
- `./scripts/conformance/conformance.sh run --filter "jsdocTypeTagCast" --verbose` - passed
- `./scripts/conformance/conformance.sh run --max 200` - passed
