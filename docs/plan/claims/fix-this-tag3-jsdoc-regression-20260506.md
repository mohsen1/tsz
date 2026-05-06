# fix(checker): report jsdoc this tag member lookup

- **Date**: 2026-05-06
- **Branch**: `fix/this-tag3-jsdoc-regression-20260506-213650`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Conformance fixes)

## Intent

Target `TypeScript/tests/cases/conformance/jsdoc/thisTag3.ts`. The canonical
picker reports an only-missing diagnostic mismatch: expected `TS2339,TS2730`,
actual `TS2730`. This slice will preserve the existing `TS2730` behavior and
restore the missing `TS2339` from the checker or solver layer that owns the
member lookup semantics.

## Files Touched

- `crates/tsz-checker/src/types/function_type.rs`
- `crates/tsz-checker/tests/jsdoc_type_expression_tests.rs`
- `docs/plan/claims/fix-this-tag3-jsdoc-regression-20260506.md`

## Verification

- `CARGO_BUILD_JOBS=2 cargo nextest run -p tsz-checker --test jsdoc_type_expression_tests jsdoc_this_tag_on_class_field_arrow_uses_lexical_this`
- `./scripts/conformance/conformance.sh run --filter "thisTag3" --verbose`
  - 1/1 passed
  - Fingerprint-only: 0
