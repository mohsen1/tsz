# [WIP] fix(solver): avoid false TS2741 in recursive conditional BuildTree

- **Date**: 2026-04-29
- **Branch**: `fix/recursive-conditional-buildtree-ts2741`
- **PR**: #1709
- **Status**: claim
- **Workstream**: Conformance (Workstream 1)

## Intent

Fix the remaining conformance divergence in
`excessPropertyCheckIntersectionWithRecursiveType.ts`, where TSZ reports an
extra TS2741 and misses a TS2339 after recursive conditional type evaluation.
Prior PR #1374 fixed mixed fixed+rest tuple inference for `Prepend`; this slice
continues from its documented follow-up around recursive conditional
instantiation / fuel behavior.

## Files Touched

- `crates/tsz-solver/src/**` (expected; exact files after diagnosis)
- `crates/tsz-checker/tests/**` or `crates/tsz-solver/tests/**` (unit
  regression test)

## Verification

- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run --package tsz-checker --lib`
- `cargo nextest run --package tsz-solver --lib`
- `./scripts/conformance/conformance.sh run --filter "excessPropertyCheckIntersectionWithRecursiveType" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`

## Investigation Notes

- `cargo nextest run --package tsz-checker --test conditional_infer_tests test_build_tree_no_false_ts2741 --run-ignored only` still fails with TS2741.
- A narrower local repro showed `PickDepth<User, 2, [any, any]>` still requires
  `children`, so the remaining bug is not the recursive `BuildTree` expansion
  or `Prepend` tuple-length inference itself. The indexed-access target
  `{ 1: T; 0: T & { children: ... } }[Length<I> extends N ? 1 : 0]` is still
  behaving as if the instantiated key selects `0` when `I` has length `2`.
- Raising evaluation depth/fuel limits did not change the failure, which makes a
  plain recursion-limit fix unlikely.
