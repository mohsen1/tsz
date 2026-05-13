# fix(solver): preserve const-asserted array element literals

- **Date**: 2026-05-13
- **Branch**: `fix-array-const-elements-20260513`
- **Base**: `upstream/main`
- **Issue**: #6112
- **PR**: https://github.com/mohsen1/tsz/pull/6119
- **Status**: wip
- **Workstream**: solver false-positive

## Intent

Fix the false TS2322 where array literals containing individually
const-asserted elements are inferred as widened primitive arrays instead of
literal-union arrays.

## Scope

- Reproduce the #6112 `["a" as const, "b" as const, "c" as const]` case
  against `tsc` and `tsz`.
- Keep implicit literal-array widening behavior intact for unasserted elements.
- Add focused checker/solver regression coverage for the const-asserted array
  element case.

## Verification Plan

- `cargo fmt`
- Focused regression test for #6112
- Related array-literal/generic-inference tests that cover widening behavior
- Manual #6112 repro comparison against `tsc` and `tsz`
