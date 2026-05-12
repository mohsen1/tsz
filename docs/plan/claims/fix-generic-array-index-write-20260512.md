# fix(checker): allow generic array indexed writes

- **Date**: 2026-05-12
- **Branch**: `fix-generic-array-index-write-20260512`
- **Base**: `main`
- **Issue**: [#6100](https://github.com/mohsen1/tsz/issues/6100)
- **PR**: draft pending
- **Status**: WIP
- **Workstream**: 1 (diagnostic conformance / false-positive checker bug)

## Intent

Make `tsz` match `tsc` for mutable generic array writes such as
`this.items[index] = value` when `items` has type `T[]`. The checker should not
emit TS2862 for a writable array indexed by `number`, while preserving TS2862
for readonly or broad generic indexed writes.

## Initial Scope

- Add a focused regression for the #6100 repro.
- Narrow the TS2862 readonly/generic-index write check so mutable arrays remain
  writable in generic contexts.
- Verify genuine TS2862 coverage still passes.

## Verification

Pending implementation.
