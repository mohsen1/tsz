# fix(checker): report jsdoc this tag member lookup

- **Date**: 2026-05-06
- **Branch**: `fix/this-tag3-jsdoc-regression-20260506-213650`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance fixes)

## Intent

Target `TypeScript/tests/cases/conformance/jsdoc/thisTag3.ts`. The canonical
picker reports an only-missing diagnostic mismatch: expected `TS2339,TS2730`,
actual `TS2730`. This slice will preserve the existing `TS2730` behavior and
restore the missing `TS2339` from the checker or solver layer that owns the
member lookup semantics.

## Files Touched

- TBD after investigation.

## Verification

- Focused Rust regression in the owning path.
- `./scripts/conformance/conformance.sh run --filter "thisTag3" --verbose`
