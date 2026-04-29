# fix(solver): avoid false TS2741 in recursive conditional BuildTree

- **Date**: 2026-04-29
- **Branch**: `fix/recursive-conditional-buildtree-ts2741`
- **PR**: #1709
- **Status**: ready
- **Workstream**: Conformance (Workstream 1)

## Intent

Fix the remaining conformance divergence in
`excessPropertyCheckIntersectionWithRecursiveType.ts`, where TSZ reports an
extra TS2741 and misses a TS2339 after recursive conditional type evaluation.
Prior PR #1374 fixed mixed fixed+rest tuple inference for `Prepend`; this slice
continues from its documented follow-up around recursive conditional
instantiation / fuel behavior.

## Files Touched

- `crates/tsz-solver/src/instantiation/instantiate.rs`
- `crates/tsz-solver/src/diagnostics/format/compound.rs`
- `crates/tsz-checker/src/error_reporter/core/diagnostic_source.rs`
- `crates/tsz-checker/tests/conditional_infer_tests.rs`

## Verification

- `cargo check --package tsz-solver`
- `cargo nextest run --package tsz-checker --test conditional_infer_tests test_generic_object_index_with_instantiated_conditional_key test_conditional_key_selects_depth_terminal_branch test_build_tree_no_false_ts2741 --run-ignored all`
- `cargo fmt --check`
- `cargo check --package tsz-checker`
- `cargo nextest run --package tsz-checker --test conditional_infer_tests`
- `CARGO_INCREMENTAL=0 cargo build --target-dir .target --profile dist-fast -p tsz-cli -p tsz-conformance`
- `.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --workers 16 --print-test --verbose --print-fingerprints --filter excessPropertyCheckIntersectionWithRecursiveType` (1/1 pass)

## Investigation Notes

- `cargo nextest run --package tsz-checker --test conditional_infer_tests test_build_tree_no_false_ts2741 --run-ignored only` still fails with TS2741.
- A narrower local repro showed `PickDepth<User, 2, [any, any]>` still requires
  `children`, so the remaining bug is not the recursive `BuildTree` expansion
  or `Prepend` tuple-length inference itself. The indexed-access target
  `{ 1: T; 0: T & { children: ... } }[Length<I> extends N ? 1 : 0]` is still
  behaving as if the instantiated key selects `0` when `I` has length `2`.
- Raising evaluation depth/fuel limits did not change the failure, which makes a
  plain recursion-limit fix unlikely.

## Resolution

The instantiator was eagerly reducing indexed-access types with a `NoopResolver`.
For a key such as `Length<I> extends N ? 1 : 0`, that can happen after `I` and
`N` are substituted but before the `Length` alias application is resolvable,
causing the conditional key to take the false branch. Resolver-dependent
indexed accesses are now deferred to the outer evaluator, which has the real
resolver and can expand `Length<I>` correctly.
