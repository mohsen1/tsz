# fix(solver): accept mapped symbol keys in Pick constraints

- **Date**: 2026-05-12
- **Branch**: `fix-symbolkeys-pick-constraint-20260512`
- **Base**: `main`
- **Issue**: [#6099](https://github.com/mohsen1/tsz/issues/6099)
- **PR**: [#6108](https://github.com/mohsen1/tsz/pull/6108)
- **Status**: WIP
- **Workstream**: 1 (diagnostic conformance / false-positive solver bug)

## Intent

Make `tsz` match `tsc` for `Pick<T, SymbolKeys<T>>` where `SymbolKeys<T>` is
a mapped type over `keyof T` that extracts symbol keys. The extracted key type
is constructed from `keyof T`, so it should satisfy `Pick`'s `keyof T`
constraint.

## Initial Scope

- Add a focused regression for the #6099 repro.
- Keep the solver change narrow to generic constraint satisfaction for mapped
  key extraction.
- Preserve existing `Pick<T, K>` constraint diagnostics when `K` is not
  provably a subset of `keyof T`.

## Verification

TBD
