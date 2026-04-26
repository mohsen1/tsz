# fix(solver): handle mixed fixed+rest params in match_rest_infer_tuple

- **Date**: 2026-04-26
- **Branch**: `claude/exciting-keller-bwwDp`
- **PR**: TBD
- **Status**: claim
- **Workstream**: Conformance (Workstream 1)

## Intent

Fix `match_rest_infer_tuple` in `infer_pattern_helpers.rs` so that
conditional-type infer patterns like `Prepend<V, T>` — which match a
function `(head: V, ...args: T) => void` against `(...args: infer R) => void`
— correctly infer `R = [V, ...T]` instead of taking the false branch.

Previously, the mixed fixed+rest case hit a `return false` path, causing
`Prepend<V, T>` to evaluate to `any` for any non-empty T.  This broke
recursive tuple-length accumulators like `BuildTree<T, N>` that use
`Prepend` to count recursion depth, producing false TS2741 errors.

Conformance target: `excessPropertyCheckIntersectionWithRecursiveType.ts`.

## Files Touched

- `crates/tsz-solver/src/evaluation/evaluate_rules/infer_pattern_helpers.rs` (~15 LOC change)
- `crates/tsz-checker/tests/conditional_infer_tests.rs` (~80 LOC added — 2 new tests)

## Verification

- `test_prepend_infer_rest_from_mixed_params` — PASS (Prepend lengths correct)
- `test_build_tree_no_false_ts2741` — PASS (no false TS2741)
- Conformance: `excessPropertyCheckIntersectionWithRecursiveType.ts`
