# perf(solver): drop redundant cache lookup in evaluate_guarded_inner

- **Date**: 2026-05-01
- **Branch**: `perf/solver-evaluate-drop-redundant-cache-lookup`
- **PR**: TBD
- **Status**: ready
- **Workstream**: §18 (Performance Targets — hot paths avoid redundant work)

## Intent

`TypeEvaluator::evaluate` (the public entry point) checks `self.cache`
at the very top and returns early on a hit, then optionally takes a
depth-check branch and finally calls `evaluate_guarded →
evaluate_guarded_inner`. The inner function historically did a second
`self.cache.get(&type_id)` before entering the recursion guard.

That second lookup was dead-on-arrival:

- `evaluate_guarded` is the only caller of `evaluate_guarded_inner`.
- `evaluate_guarded` is only called from `evaluate` (lines 438 and 443),
  both *after* the entry-point cache check at line 411.
- `&mut self` is held exclusively for the whole call.
- `stacker::maybe_grow` runs its closure synchronously on a grown
  stack — no deferral.

So the inner lookup was guaranteed to miss for every non-cached input
that reached it. Removing it skips one HashMap.get per non-cached
`evaluate` call — small but pure, on a very hot path.

## Files Touched

- `crates/tsz-solver/src/evaluation/evaluate.rs` (remove the redundant
  lookup; document the invariant in its place so future readers don't
  reinstate it defensively).

## Verification

- `cargo nextest run -p tsz-checker -p tsz-solver --lib` (run from a
  fresh worktree built off latest origin/main) — all green.
- Pure perf refactor — no behavioural change.
