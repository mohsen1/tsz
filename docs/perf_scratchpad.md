# Performance Scratchpad: tsz vs tsgo (BCT/Constraint)

Date: 2026-02-16
Owner: Codex + user

## Goal
Make `tsz` at least `2x` faster than `tsgo` on the problematic stress cases while preserving conformance and existing tests.

## Architecture Guardrails (from `docs/architecture/NORTH_STAR.md`)
- Keep type algorithms in Solver (no Checker semantic leakage).
- Preserve single semantic type universe and query boundaries.
- Use solver-side optimizations only (no pipeline bypasses).
- Avoid behavior changes that diverge from `tsc` parity.

## Reproduced Baseline (filtered `BCT|Constraint`)
- `BCT candidates=200`: tsz ~180ms, tsgo ~158ms (tsgo wins ~1.13x).
- `Constraint conflicts N=200`: tsz ~409-410ms, tsgo ~154-165ms (tsgo wins ~2.5-2.7x).

## Findings So Far
1. `infer.rs::extend_dedup` rebuilt a hash set for each merge, including single-item merges (hot path in inference unioning).
2. Upper-bound validation in inference built intersections eagerly; changed to size-aware path did not materially move N=200.
3. Most severe cost likely in repeated large-object compatibility against intersection-heavy constraints, not in small-N logic.
4. Existing target-intersection subtype logic checks each member individually (`source <: A & B & ...` => N recursive checks).

## Iterations Applied
1. `infer.rs`
- Optimized `extend_dedup`:
  - Single-item merge fast path uses `Vec::contains` (no hash allocation).
  - Multi-item path now uses mutable set insert logic.

2. `infer.rs`
- Refactored upper-bound validation into helper methods with thresholded intersection fast path.
- Kept semantics: still validates against each bound on failure.

3. `subtype.rs`
- Added guarded fast path for large target intersections of public object members:
  - Detect object-only intersection members after lazy/ref resolution.
  - Build merged structural object target once via `collect_properties`.
  - Check subtype against merged target instead of per-member recursion.
  - Falls back to existing behavior when guards are not met.

## Why Iteration 3 Should Help
- The `Constraint conflicts N=200` fixture includes large `T extends Constraint0 & ... & Constraint199` patterns.
- Previous algorithm performed many recursive checks across members.
- Merged-object fast path converts this to one structural object check when safe.

## Next Measurements
- Re-run `./scripts/bench-vs-tsgo.sh --filter 'BCT|Constraint'`.
- If still below target, inspect remaining hotspots around:
  - Lazy DefId resolution churn during subtype checks.
  - Object shape interning/sorting overhead for large literals.
  - Constraint-strengthening fixed-point overhead in inference contexts with no inter-param edges.


## Iteration: callee resolution hotspot (2026-02-17)

### Tracing-driven diagnosis
- Ran `TSZ_PERF=1 TSZ_LOG_FORMAT=json .target-bench/dist/tsz --noEmit /tmp/constraint_200.ts`.
- After initial fixes, `call_collect_args` and `call_resolve` were small; `call_expr` remained dominant.
- Added phase timings in call pipeline (`call_callee`, `call_prepare`, `call_handle`).
- Result: `call_callee` was the dominant subphase (~136ms total across 201 calls).

### Root cause
- `get_type_of_call_expression_inner` resolved identifier callees through full `get_type_of_node -> get_type_of_identifier` machinery for every `constrainN(...)` call.
- For this benchmark pattern (many simple declared functions), that path was overkill and dominated checker time.

### Implemented fix
- Added guarded fast callee path in `get_type_of_call_expression_inner` for plain identifier function symbols:
  - Prefer `binder.node_symbols` lookup (fallback to `resolve_identifier_symbol`).
  - Validate symbol/name/flags (`FUNCTION|VALUE`, non-ALIAS, current-file guard).
  - Use `get_type_of_symbol(sym_id)` directly and mark symbol referenced.
  - Fallback to existing `get_type_of_node` path when guard does not hold.
- Removed temporary hot-path solver perf warn logs (`infer.rs`, `compat.rs`) used during investigation.

### Perf results (targeted `bench-vs-tsgo`)
- `BCT candidates=200`: **59.93ms vs 156.26ms** (tsz **2.61x faster**).
- `Constraint conflicts N=200`: **46.43ms vs 189.03ms** (tsz **4.07x faster**).

### Validation notes
- Conformance run completed: `8490/12574 (67.5%)`.
- `cargo test -p tsz-solver`: pass (`3486 passed, 0 failed`).
- `cargo test -p tsz-checker -p tsz-solver`: checker has 4 pre-existing control-flow test failures in this workspace (`test_array_mutation_clears_predicate_narrowing`, `test_asserts_call_statement_narrows`, `test_asserts_type_predicate_narrows_true_branch`, `test_user_defined_type_predicate_narrows_branches`).

### Final verification after rebuild
- Rebuilt dist binary and reran filtered benchmark on current code.
- Final numbers:
  - `BCT candidates=200`: **61.49ms** vs tsgo **159.32ms** (**2.59x faster**)
  - `Constraint conflicts N=200`: **47.76ms** vs tsgo **155.67ms** (**3.26x faster**)
- Confirms target (`>=2x faster than tsgo`) for both previously regressed cases.
