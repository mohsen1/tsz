# perf(solver): terminal-kind fast path in ContainsTypeChecker shared visitor

- **Date**: 2026-05-01
- **Branch**: `perf/solver-contains-type-checker-terminal-fast-path`
- **PR**: TBD
- **Status**: ready
- **Workstream**: §18 (Performance Targets — hot paths avoid per-op heap allocation)

## Intent

`ContainsTypeChecker.check` is the shared engine behind
`contains_type_parameters`, `contains_infer_types`, `contains_any_type`,
and several other recursive predicates (433 call sites across
solver/checker). The pre-existing flow always paid the
`guard.enter`/`guard.leave` HashSet round-trip even for terminal kinds
that have no children to walk — the recursive walker's leaf arm would
then immediately return `false` for `Literal`/`Lazy`/`TypeQuery`/etc.

This PR mirrors the iter-5 fix in `type_contains_infer` at the shared
visitor level: after the predicate check, if the key matches a
terminal kind that the walker's leaf arm returns `false` for, skip
the guard bookkeeping entirely and just memoize `false`. No cycle risk
(no children = no recursion), no extra HashSet ops.

The terminal-kind set is taken verbatim from the walker's leaf arm in
`check_key`, minus `TypeData::Intrinsic(_)` which is already handled
by the entry-level `is_intrinsic()` check.

## Files Touched

- `crates/tsz-solver/src/visitors/visitor_predicates.rs`
  (~30 LOC: terminal-kind fast path + reordering of `lookup`/`predicate`
  before guard).

## Verification

- `cargo nextest run -p tsz-checker -p tsz-solver --lib` → **8666/8666 pass**.
- Pure perf refactor: terminal kinds always returned `false` from
  `check_key`'s leaf arm anyway, so behaviour is unchanged.
- 433 call sites benefit; the dominant input shapes for these predicates
  are `Lazy(DefId)` (generic interface refs before evaluation) and
  `Literal(_)` (concrete constants), both of which are now in the fast
  path.
