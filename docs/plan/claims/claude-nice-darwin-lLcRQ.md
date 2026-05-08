# fix(checker): tighten generic-ref scoped-param TS2344 skip

- **Date**: 2026-05-08 11:07:55
- **Branch**: `claude/nice-darwin-lLcRQ`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance (TS2344 false negatives)

## Intent

Closes #3063. The constraint-validation skip in
`crates/tsz-checker/src/checkers/generic_checker/constraint_validation.rs`
currently bails out when a type argument is a generic reference that
mentions a scoped type parameter (e.g. `Box<Array<U>>`), even when the
constraint is a concrete primitive like `string`. tsc still validates
those instantiations and reports TS2344. Tighten the skip to only fire
when the resolved generic-ref instantiation is not assignable-checkable
against a concrete constraint.

## Files Touched

- `crates/tsz-checker/src/checkers/generic_checker/constraint_validation.rs`
- `crates/tsz-checker/src/checkers/generic_checker/tests.rs` (new unit test)

## Verification

- `cargo nextest run -p tsz-checker -p tsz-solver`
- `./scripts/conformance/conformance.sh run --filter "<targeted>"`
- Repro from the issue exits with TS2344 for `BadArray`/`BadPromise`/`BadRecord`.
