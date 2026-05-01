# perf(solver): terminal-kind fast path in contains_type_parameter_named_shallow iterative walker

- **Date**: 2026-05-01
- **Branch**: `perf/solver-shallow-walker-terminal-fast-path`
- **PR**: TBD
- **Status**: ready
- **Workstream**: §18 (Performance Targets — hot paths avoid redundant work)

## Intent

Completes the iter-5 → iter-7 → iter-9 series by applying the same
terminal-kind fast path to the **iterative** walker
`contains_type_parameter_named_shallow`. The earlier PRs (#1978,
#1988, #1995) all targeted the *recursive* `check`-method walkers
(`type_contains_infer`, `ContainsTypeChecker`, `FreeTypeParamChecker`,
`FreeInferChecker`). This walker uses an explicit
`Vec<TypeId>` stack instead of recursion but has the same shape
issue: terminal-kind types reach the dispatch at the bottom of the
loop body, where `for_each_child_by_id` iterates an empty child set,
costing closure setup and visitor dispatch for nothing.

## What this saves

For every popped terminal-kind type:
- One `for_each_child_by_id` call (closure construction + visitor
  dispatch over an empty children iterator).

The terminal-kind set is taken verbatim from the leaf arms of the
three recursive walkers I touched in prior PRs, so it stays
behaviourally identical to those siblings.

## Files Touched

- `crates/tsz-solver/src/visitors/visitor_predicates.rs`
  (extend the per-pop fast path with a terminal-kind branch in
  `contains_type_parameter_named_shallow`).

## Verification

- `cargo nextest run -p tsz-checker -p tsz-solver --lib` → 8673/8673 pass.
- Pure perf refactor — terminal kinds have no children to enumerate,
  so the skipped `for_each_child_by_id` was already a no-op.
