# fix(solver): infer constrained conditional types

- **Date**: 2026-05-13
- **Branch**: `fix-constrained-infer-conditional-20260513`
- **Base**: `upstream/main`
- **Issue**: #6121
- **PR**: TBD
- **Status**: WIP
- **Workstream**: solver conformance

## Intent

Fix the false TS2322 where `infer U extends Constraint` patterns inside
conditional types evaluate to `never` even when the inferred type satisfies the
constraint.

## Scope

- Reproduce the #6121 array-element constrained infer case against `tsc` and
  `tsz`.
- Use DeepWiki for source-code research and save the full response outside the
  repository.
- Keep the fix focused on binding constrained infer candidates only when the
  candidate satisfies the declared constraint.
- Add focused checker/solver coverage for matching and non-matching constrained
  infer patterns.

## Verification Plan

- `cargo fmt`
- Focused constrained-infer conditional regression tests
- Related conditional-evaluation and assignability tests
- Manual #6121 repro comparison against `tsc` and `tsz`
