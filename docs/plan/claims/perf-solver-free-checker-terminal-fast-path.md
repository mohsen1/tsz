# perf(solver): terminal-kind fast path in FreeTypeParamChecker and FreeInferChecker

- **Date**: 2026-05-01
- **Branch**: `perf/solver-free-checker-terminal-fast-path`
- **PR**: TBD
- **Status**: ready
- **Workstream**: §18 (Performance Targets — hot paths avoid per-op heap allocation)

## Intent

Generalises the iter-7 `ContainsTypeChecker` fast path (PR #1988) to the
two adjacent recursive walkers in the same module: `FreeTypeParamChecker`
(behind `contains_free_type_parameters`) and `FreeInferChecker` (behind
`contains_free_infer_types`).

For each walker, the leaf arm of `check_key` returns `false` for a fixed
set of terminal kinds — types with no children to walk and no cycle
risk. The pre-existing flow paid the `guard.enter`/`guard.leave`
HashSet round-trip on the entry path even when the input was one of
those terminal kinds. Promoting the set to the entry-point fast path
matches what `ContainsTypeChecker` now does.

The terminal-kind set differs slightly per walker:

- `FreeTypeParamChecker`: `Literal | Error | Lazy | Recursive | TypeQuery | UniqueSymbol | ModuleNamespace | UnresolvedTypeName`. Excludes `Intrinsic` (handled by `is_intrinsic()`), `ThisType`, and `BoundParameter` (the predicate above already returns `true` for those).
- `FreeInferChecker`: same set, *plus* `TypeParameter | ThisType | BoundParameter` — this checker deliberately does not walk into TypeParameter constraints (treating definitional infer as bound), so any TypeParameter at the entry has no children to visit.

## Files Touched

- `crates/tsz-solver/src/visitors/visitor_predicates.rs`
  (`FreeTypeParamChecker.check` and `FreeInferChecker.check`).

## Verification

- `cargo nextest run -p tsz-checker -p tsz-solver --lib` → **8670/8670 pass**.
- Pure perf refactor: terminal kinds always returned `false` from
  `check_key`'s leaf arm anyway, so behaviour is unchanged.
