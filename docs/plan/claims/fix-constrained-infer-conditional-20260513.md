# fix(solver): infer constrained conditional types

- **Date**: 2026-05-13
- **Branch**: `fix-constrained-infer-conditional-20260513`
- **Base**: `upstream/main`
- **Issue**: #6121
- **PR**: https://github.com/mohsen1/tsz/pull/6125
- **Status**: Implemented; local verification complete
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

## Result

- Fixed the false `TS2322` in #6121 by letting array conditional infer
  evaluation recover element candidates from expanded `Array<T>` object shapes.
- Avoided treating every one-argument generic application as an array by only
  accepting known array applications and deferring unresolved lazy application
  bases until later expansion can produce a structural array shape.
- Preserved rejecting non-array applications and primitive element types that do
  not satisfy `infer U extends object`.
- DeepWiki source research was saved outside the repository:
  `/tmp/tsz-deepwiki-6121.md`.

## Verification

- `cargo fmt`
- `git diff --check`
- `npm exec --yes --package typescript tsc -- --noEmit --strict /tmp/tsz-6121-repro.ts`
- `env CARGO_INCREMENTAL=0 cargo run -q -p tsz-cli --bin tsz -- --noEmit --strict /tmp/tsz-6121-repro.ts`
- `.target/release/tsz --noEmit --strict /tmp/tsz-6121-repro.ts`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --test infer_extends_constraint_substitution_tests -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-solver test_conditional_infer_array_element -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-solver --lib`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --lib`
- `env CARGO_INCREMENTAL=0 cargo build --release -p tsz-cli --bin tsz`
