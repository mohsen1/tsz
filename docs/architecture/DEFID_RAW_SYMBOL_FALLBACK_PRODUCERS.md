# DefId Raw-Symbol Fallback Producer Map

**Status**: Audit for #7029
**Scope**: `Lazy(DefId)` fallback paths that still recover from
`interner.reference(SymbolRef(N))`-style construction.

`TypeInterner::reference(SymbolRef)` is deprecated because it wraps the raw
`SymbolId` value as `DefId(symbol.0)`. `SymbolId` and `DefId` are independent
identity spaces, so the resulting lazy type can only resolve when a resolver
recognizes the raw-symbol shape and redirects it through the real
symbol-to-definition mapping.

This map is behavior-preserving. It records the current source-linked producer
and resolver sites so future migration slices can remove one producer at a time.

## Resolver Fallbacks

| Site | Current behavior | Migration note |
|---|---|---|
| `crates/tsz-checker/src/context/resolver.rs` `CheckerContext::resolve_lazy` | If `def_symbol_identity(def_id)` misses, treats `DefId.0` as a candidate `SymbolId`, checks `DefinitionStore::find_def_by_symbol`, and resolves through the real symbol identity. | Keep until all checker producers avoid `interner.reference(SymbolRef)` fallback. This is the primary compatibility path described by #7027. |
| `crates/tsz-solver/src/def/resolver.rs` `TypeEnvironment::resolve_lazy` | If `get_def(def_id)` misses, treats the raw `DefId.0` as a symbol key into `symbol_to_def`, then resolves the real definition body or class instance type. | This is the solver-side type-environment compatibility path. `TSZ_PERF_COUNTERS` exposes `identity.type_environment_raw_symbol_lazy_fallbacks`, and trace logging includes raw and redirected IDs. |
| `crates/tsz-solver/src/def/resolver.rs` `TypeEnvironment::symbol_to_def_id` | Falls back to the shared `DefinitionStore` when local `symbol_to_def` is missing. | This is a stabilizing lookup, not a raw producer; it reduces the need for caller-side raw fallback construction. |

## Active Checker Producers

| Producer | Fallback shape | Why it still exists | Narrow migration path |
|---|---|---|---|
| `crates/tsz-checker/src/flow/control_flow/type_guards.rs` `check_array_buffer_is_view` | `resolve_symbol_to_lazy(symbol_ref).unwrap_or_else(|| interner.reference(symbol_ref))` for `ArrayBufferView` and `ArrayBufferLike`. | Manual predicate construction runs only when the signature predicate did not provide a resolved type. The preferred `TypeEnvironment` path can still be absent in that fallback branch. | Make the manual branch require a real `DefId` for both lib symbols, or plumb a pre-resolution step that guarantees `TypeEnvironment::symbol_to_def_id` before constructing the predicate type. |
| `crates/tsz-checker/src/flow/control_flow/narrowing.rs` `instance_type_from_constructor` | `resolve_symbol_to_lazy(symbol_ref).unwrap_or_else(|| interner.reference(symbol_ref))` for class constructor symbols. | `instanceof` narrowing can fall back to binder symbol resolution when the expression type path cannot recover an instance type. | Replace the raw fallback with explicit DefId stabilization for the constructor symbol before returning a lazy instance type. |
| `crates/tsz-checker/src/flow/control_flow/narrowing.rs` `instance_type_from_constructor` | Same fallback for global constructor variables with `INTERFACE | VARIABLE`. | Lib/global constructor symbols may be available as binder symbols before a type-environment mapping is visible to this helper. | Share the same explicit DefId stabilization path as class constructor symbols, with a focused lib/global constructor regression. |

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
- `crates/tsz-checker/src/flow/control_flow/narrowing.rs` later type-predicate
  branches use `resolve_symbol_to_lazy` directly without raw fallback.
- `crates/tsz-solver/src/relations/subtype/rules/objects.rs` and
  `crates/tsz-solver/src/operations/property.rs` explicitly avoid
  `interner.reference(symbol_ref)` when nominalizing receivers; they keep the
  structural object when no real `DefId` mapping exists.

## Suggested Follow-Up Order

1. Use `identity.type_environment_raw_symbol_lazy_fallbacks` plus
   `tsz::solver::def_id` traces to confirm runtime frequency and call stacks.
2. Migrate one `instance_type_from_constructor` branch under #7030, because the
   branch is small and already has clear class/global-constructor predicates.
3. Migrate the `ArrayBuffer.isView` manual fallback after confirming lib symbol
   DefIds are stable in that path.
4. Tighten the guard from #7031 once the raw `.reference(...)` budget decreases.
