# fix(checker): allow assertion predicates with narrowing intersections

- **Date**: 2026-05-12
- **Branch**: `fix-assertion-intersection-predicate-20260512`
- **Base**: `main`
- **Issue**: [#6082](https://github.com/mohsen1/tsz/issues/6082)
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (diagnostic conformance / false-positive checker bug)

## Intent

Make `tsz` match `tsc` for assertion functions whose predicate narrows a
parameter to an intersection of the parameter type and a stricter object shape.
The predicate type should be accepted when it is assignable to the parameter
type.

## Planned Scope

- Add focused checker regression coverage for `asserts d is Data & { ... }`.
- Fix TS2677 predicate assignability so narrowing intersections are accepted.
- Keep genuine widening predicates rejected.

## Verification Plan

- Targeted checker regression test for #6082.
- Focused checker test file or crate-level checker tests covering predicate
  diagnostics.
- `env CARGO_INCREMENTAL=0 cargo check -p tsz-checker`
- Manual #6082 repro comparison against `tsc` and `tsz`.
