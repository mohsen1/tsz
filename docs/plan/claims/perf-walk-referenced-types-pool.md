# perf(solver): pool walk_referenced_types scratch buffers

- **Date**: 2026-05-09
- **Branch**: `perf/walk-referenced-types-pool`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T3.3 (small-fixture polish — visitor allocator reuse)

## Intent

`walk_referenced_types` at
`crates/tsz-solver/src/visitors/visitor.rs:568` allocated a fresh
`FxHashSet<TypeId>` and `Vec<TypeId>` on every call, plus the
incremental reallocations as the type graph grew. The function is on
the hot path for `collect_lazy_def_ids`,
`contains_concrete_application_with_def`, `collect_enum_def_ids`,
`collect_type_queries`, and several other crawlers — each invocation
pays the allocator round-trip plus 2–4 grows.

Pool the visited-set and stack in a thread-local `RefCell<Option<...>>`.
Each call `take()`s the pool, clears it, uses it, and puts it back
keeping the larger allocation. Reentrant calls (when `f` itself calls
`walk_referenced_types`) fall through to fresh allocations because the
slot is already empty.

Per `docs/plan/PERFORMANCE_PLAN.md` §6.3.

## Files Touched

- `crates/tsz-solver/src/visitors/visitor.rs` (~25 LOC change at line ~568)

## Verification

- `cargo check -p tsz-solver` (clean)
- `cargo nextest run -p tsz-solver --lib` (5713 passed, 7 skipped)
- Behavior is byte-identical: same `visited` set, same DFS over the same
  `TypeId` graph, same `f` invocation order. The only change is buffer
  reuse instead of fresh allocation.
