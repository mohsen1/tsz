# DefId Raw-Symbol Fallback Producer Map

**Status**: Updated after #7717, #7756, and #7758
**Scope**: `Lazy(DefId)` fallback paths that still recover from
`interner.reference(SymbolRef(N))`-style construction.

`TypeInterner::reference(SymbolRef)` is deprecated because it wraps the raw
`SymbolId` value as `DefId(symbol.0)`. `SymbolId` and `DefId` are independent
identity spaces, so the resulting lazy type can only resolve when a resolver
recognizes the raw-symbol shape and redirects it through the real
symbol-to-definition mapping.

This map is behavior-preserving. It records the current resolver compatibility
paths, deprecated wrapper APIs, and migrated producer sites so future slices can
keep the raw-symbol fallback budget from growing again.

## Resolver Fallbacks

| Site | Current behavior | Migration note |
|---|---|---|
| `crates/tsz-checker/src/context/resolver.rs` `CheckerContext::resolve_lazy` | If `def_symbol_identity(def_id)` misses, treats `DefId.0` as a candidate `SymbolId`, checks `DefinitionStore::find_def_by_symbol`, and resolves through the real symbol identity. | Legacy compatibility path. Non-test checker sources now have a zero raw `.reference(...)` construction budget, so any hit should come from deprecated wrappers, tests, or non-checker callers. |
| `crates/tsz-solver/src/def/resolver.rs` `TypeEnvironment::resolve_lazy` | If `get_def(def_id)` misses, treats the raw `DefId.0` as a symbol key into `symbol_to_def`, then resolves the real definition body or class instance type. | This is the solver-side type-environment compatibility path. `TSZ_PERF_COUNTERS` exposes `identity.type_environment_raw_symbol_lazy_fallbacks`, and trace logging includes raw and redirected IDs. |
| `crates/tsz-solver/src/def/resolver.rs` `TypeEnvironment::symbol_to_def_id` | Falls back to the shared `DefinitionStore` when local `symbol_to_def` is missing. | This is a stabilizing lookup, not a raw producer; it reduces the need for caller-side raw fallback construction. |

## Active Checker Producers

No active non-test checker producers remain. The architecture guard
`test_checker_raw_symbol_reference_construction_budget` allows zero raw
`.reference(...)` construction calls in checker sources, and focused guards cover
the previously migrated `instanceof` and `ArrayBuffer.isView` branches.

## Deprecated API Surface

These APIs can create raw-shaped lazy types, but they are wrappers rather than
business-logic producers by themselves:

- `crates/tsz-solver/src/intern/core/constructors.rs` `TypeInterner::reference`
- `crates/tsz-solver/src/intern/type_factory.rs` `TypeFactory::reference`
- `crates/tsz-solver/src/caches/db.rs` `TypeDatabase::reference`
- `crates/tsz-solver/src/caches/query_cache.rs` `QueryCache::reference`

New checker code should use a real `DefId` and `lazy(def_id)` instead. Tests may
still use the deprecated constructor when they explicitly need to exercise the
compatibility fallback.

## Already-Migrated Or Avoided Sites

The following nearby paths should not be counted as remaining producers:

- `crates/tsz-checker/src/flow/control_flow/comparison_types.rs` uses
  `resolve_symbol_to_lazy` for symbol comparison types and returns `None` when a
  real mapping is unavailable.
- `crates/tsz-checker/src/flow/control_flow/assignment_fallback.rs` uses
  `resolve_symbol_to_lazy(SymbolRef(...))` before resolving through the active
  environment.
- `crates/tsz-checker/src/flow/control_flow/type_guards.rs`
  `check_array_buffer_is_view` now requires real `DefId`-backed lazy refs for
  both `ArrayBufferView` and `ArrayBufferLike` before constructing the manual
  predicate fallback.
- `crates/tsz-checker/src/flow/control_flow/narrowing.rs`
  `instance_type_from_constructor` now requires real `DefId`-backed lazy refs for
  class symbols and global constructor variables with `INTERFACE | VARIABLE`.
- `crates/tsz-checker/src/flow/control_flow/narrowing.rs` later type-predicate
  branches use `resolve_symbol_to_lazy` directly without raw fallback.
- `crates/tsz-solver/src/relations/subtype/rules/objects.rs` and
  `crates/tsz-solver/src/operations/property.rs` explicitly avoid
  `interner.reference(symbol_ref)` when nominalizing receivers; they keep the
  structural object when no real `DefId` mapping exists.

## Suggested Follow-Up Order

1. Use `identity.type_environment_raw_symbol_lazy_fallbacks` plus
   `tsz::solver::def_id` traces on project-corpus runs to confirm whether the
   resolver compatibility path still fires.
2. If the counter is nonzero, group hits by caller and migrate the remaining
   deprecated wrapper or non-checker producer instead of widening checker
   fallbacks.
3. Once runtime hits are understood, narrow or retire the checker/solver
   `resolve_lazy` raw-symbol compatibility paths.
4. Keep the zero-budget architecture guard in place so new checker code cannot
   reintroduce raw `SymbolRef` lazy construction.
