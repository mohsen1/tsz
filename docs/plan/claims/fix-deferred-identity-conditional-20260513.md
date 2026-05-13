# fix(solver): accept distributive identity conditional return

- **Date**: 2026-05-13
- **Branch**: `fix-deferred-identity-conditional-20260513`
- **Base**: `upstream/main`
- **Issue**: #6064
- **PR**: https://github.com/mohsen1/tsz/pull/6124
- **Status**: wip
- **Workstream**: solver false-positive

## Intent

Fix the false TS2322 where `T` is rejected as not assignable to a transparent
conditional alias like `T extends unknown ? T : never`.

## Scope

- Reproduce the #6064 `Deferred<T>` case against `tsc` and `tsz`.
- Identify the smallest solver/checker path that should treat transparent
  identity conditionals as assignable without weakening unrelated conditional
  failures.
- Add focused regression coverage for the generic return assignment.

## Verification Plan

- `cargo fmt`
- Focused #6064 regression test
- Related conditional/assignability tests covering deferred conditional types
- Manual #6064 repro comparison against `tsc` and `tsz`
