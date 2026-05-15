# Instantiation Cache

`InstantiationCache` is an active per-`QueryCache` memoization layer for
cache-aware type-instantiation entry points. It is not a `TypeInterner` cache:
the cache is cleared by `QueryCache::clear()`, counted in
`QueryCacheStatistics`, and disabled automatically for callers that only have a
raw `TypeDatabase`.

## Ownership

- Storage: `crates/tsz-solver/src/caches/instantiation_cache.rs`
- Statistics and invalidation: `crates/tsz-solver/src/caches/query_cache.rs`
- Trait boundary and raw-database defaults: `crates/tsz-solver/src/caches/db.rs`
- Production probes and inserts: `crates/tsz-solver/src/caches/instantiation_cache.rs`
- Focused coverage: `crates/tsz-solver/src/caches/instantiation_cache_test.rs`

## Active Probe And Insert Sites

The cache is only used when an entry point receives `Some(&dyn QueryDatabase)`.
Each site probes before constructing `TypeInstantiator` and inserts only
successful non-depth-overflow results.

| Entry point | Key mode | Notes |
| --- | --- | --- |
| `instantiate_type_cached` | `0` | Default substitution walk. Leaf `TypeParameter` and `IndexAccess` fast paths return before key construction. |
| `instantiate_type_preserving_cached` | `MODE_PRESERVE_UNSUBSTITUTED` | Keeps unsubstituted type parameters instead of falling back to constraints. |
| `instantiate_type_preserving_meta_cached` | `MODE_PRESERVE_META` | Preserves meta structures such as `keyof`, index access, and mapped types. |
| `instantiate_type_with_infer_cached` | `MODE_SUBSTITUTE_INFER` | Substitutes infer variables instead of ordinary type parameters. |
| `substitute_this_type_cached` | `MODE_PRESERVE_UNSUBSTITUTED`, `this_type = Some(_)` | Uses an empty substitution plus receiver-specific `this_type`. |
| `substitute_this_type_at_return_position` | `MODE_PRESERVE_UNSUBSTITUTED | MODE_SHALLOW_THIS_ONLY`, `this_type = Some(_)` | Shallow return-position `this` substitution. Must not alias the deep `this` slot. |

The plain compatibility entry points, including `instantiate_type` and
`substitute_this_type`, pass `None` and do not touch the cache. Direct internal
recursive calls that need depth status also avoid the cache.

## Key Correctness

The key shape is:

```text
(TypeId, CanonicalSubst, mode_bits, Option<this_type>) -> TypeId
```

`TypeId` is the source type body. It is required because identical substitutions
can be applied to unrelated bodies.

`CanonicalSubst` is the sorted `(Atom, TypeId)` substitution payload. Sorting by
`Atom` makes equal substitutions compare equal even when their source
`FxHashMap` insertion order differs. The payload is stored directly in the key
instead of being interned because `TypeInterner` outlives `QueryCache::clear()`.

`mode_bits` is part of the key because instantiator flags change the walk shape
and can produce different results for the same source body and substitution.
The currently used bits are `MODE_SUBSTITUTE_INFER`, `MODE_PRESERVE_META`,
`MODE_PRESERVE_UNSUBSTITUTED`, and `MODE_SHALLOW_THIS_ONLY`.

`this_type` is part of the key because `substitute_this_type_cached` intentionally
uses an empty substitution. Without the receiver slot, substituting the same
body for two receiver types would alias.

## Invalidation And Stats

`QueryCache::clear()` clears the instantiation cache and leaves the
`TypeInterner` intact. That boundary is the reason this cache belongs on
`QueryCache`, not on `TypeInterner`.

`QueryCache::lookup_instantiation_cache` records hits and misses in
`QueryCacheStatistics`. Inserts update the entry count through
`InstantiationCache::len()`. Trait defaults in `QueryDatabase` always miss and
do not update counters, so tests or raw database callers do not report
misleading cache activity.

Existing focused tests cover hit-after-miss behavior, distinct substitutions,
receiver-specific `this_type`, shallow-vs-deep `this` mode separation,
fast-path non-caching, `query_db = None`, mode-bit isolation, and
`QueryCache::clear()`.
