# fix(solver): handle mixed fixed+rest params in match_rest_infer_tuple

- **Date**: 2026-04-26
- **Branch**: `claude/exciting-keller-bwwDp`
- **PR**: #1374
- **Status**: ready
- **Workstream**: Conformance (Workstream 1)

## Intent

Fix `match_rest_infer_tuple` in `infer_pattern_helpers.rs` so that
conditional-type infer patterns like `Prepend<V, T>` — which match a
function `(head: V, ...args: T) => void` against `(...args: infer R) => void`
— correctly infer `R = [V, ...T]` instead of taking the false branch.

Previously, the mixed fixed+rest case hit a `return false` path, causing
`Prepend<V, T>` to evaluate to `any` for any non-empty T.

## Fix

Removed the early `return false` branch and unified the all-fixed and
mixed fixed+rest paths into a single `let tuple_elems` builder that
preserves each param's `rest` flag. This yields a variadic tuple
`[head, ...rest]` whose `fixed_length()` correctly walks into the rest
element.

## Files Touched

- `crates/tsz-solver/src/evaluation/evaluate_rules/infer_pattern_helpers.rs`
  (+15/-2 LOC)
- `crates/tsz-checker/tests/conditional_infer_tests.rs` (+86 LOC, 2 new tests)
- `crates/tsz-checker/Cargo.toml` (+4 LOC: register `conditional_infer_tests`
  test target — required because `autotests = false`)

## Verification

- `cargo nextest run -p tsz-checker --test conditional_infer_tests` —
  6 passed, 1 ignored.
- `cargo nextest run -p tsz-solver --lib` — 5514 passed.
- `cargo nextest run -p tsz-checker --lib` — 2887 passed.
- `test_prepend_infer_rest_from_mixed_params` confirms
  `Prepend<any, []>` has length 1 and `Prepend<any, [any]>` has length 2.

## Follow-ups

`test_build_tree_no_false_ts2741` is marked `#[ignore]` because
recursive conditional-type instantiation (BuildTree depth N) still
emits TS2741 even though the underlying `Prepend` infer now produces
the correct `[head: any, ...args: [...]]` shape (visible in
conformance verbose output for
`excessPropertyCheckIntersectionWithRecursiveType.ts`). The remaining
issue is unrelated to `match_rest_infer_tuple` — it sits inside the
conditional-type recursion / fuel limiter and warrants a separate PR.
