# fix(checker): suppress TS9006 when JSDoc `typeof import("/...")` is unresolvable

- **Date**: 2026-04-26
- **Branch**: `fix/ts9006-suppress-when-import-resolution-failed`
- **PR**: #1472
- **Status**: ready
- **Workstream**: 1 (conformance)

## Intent

`jsDeclarationsTypeReassignmentFromDeclaration.ts` expects only TS2307 for an
unresolvable absolute-path JSDoc import (`/** @type {typeof import("/some-mod")} */`)
but tsz also fired TS9006 ("Declaration emit for this file requires using
private name 'Item' from module 'some-mod'"). tsz's resolver still finds the
file via basename probing even though tsc considers `/some-mod` unresolvable,
so the type carries `Item` and the private-name walk in
`first_private_name_from_external_module_reference` reports it.

The fix is to suppress the TS9006 path when the current file references the
target file via a JSDoc `typeof import(<spec>)` whose `<spec>` matches the
same unresolvable predicate the JSDoc diagnostic check uses (rooted/absolute
paths with no ambient module fallback). Stacking TS9006 on top of TS2307 for
the same module just contradicts the prior error.

## Files Touched

- `crates/tsz-checker/src/state/variable_checking/variable_helpers/declaration_emit.rs`
  (~75 LOC: new helper `current_file_jsdoc_typeof_import_unresolvable_for_target`,
  one-line guard inside `private_external_module_nameability_info`)
- `crates/tsz-checker/tests/js_jsdoc_diagnostics_tests.rs`
  (~40 LOC: regression test `checked_js_jsdoc_type_with_unresolvable_module_does_not_emit_ts9006`)

## Verification

- `cargo nextest run -p tsz-checker --test js_jsdoc_diagnostics_tests` (8/8 pass)
- `./scripts/conformance/conformance.sh run --filter "jsDeclarations"` (96/96 pass)
- `./scripts/conformance/conformance.sh run --filter "jsDeclarationsTypeReassignmentFromDeclaration"` (2/2 pass)
