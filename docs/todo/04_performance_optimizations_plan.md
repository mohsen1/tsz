# Performance Optimizations - Remaining Work

**Updated**: January 31, 2026  
**Status**: Most optimizations complete. tsz wins 29/31 benchmarks vs tsgo.

---

## Current Benchmark Results

```
Test                                            Lines     KB    tsz(ms)   tsgo(ms)   Winner   Factor
─────────────────────────────────────────────────────────────────────────────────────────────────
largeControlFlowGraph.ts                        10005    136      25.99     588.71      tsz   22.65x
conditionalTypeDiscriminatingLargeUnionRe...     8011    136      15.92      94.06      tsz    5.91x
unionSubtypeReductionErrors.ts                   6011    112      17.69      86.98      tsz    4.92x
manyConstExports.ts                              5002    150     222.95      92.77     tsgo    2.40x
binderBinaryExpressionStress.ts                  4971     38      14.14      97.46      tsz    6.89x
binderBinaryExpressionStressJs.ts                4973     39      14.28      83.46      tsz    5.84x
enumLiteralsSubtypeReduction.ts                  2054     39      16.45      91.24      tsz    5.55x
200 classes                                      9203    162     110.63      86.27     tsgo    1.28x
200 generic functions                            5011    164      93.95     111.66      tsz    1.19x
200 union members                                 489     24      25.11     133.79      tsz    5.33x
─────────────────────────────────────────────────────────────────────────────────────────────────
Score: tsz 29 vs tsgo 2
```

---

## Completed Optimizations ✅

| Optimization | Impact | Commit |
|-------------|--------|--------|
| Parallel lib file loading | 9x startup improvement | `483e3a5ce` |
| Parallel lib parsing (rayon) | Multi-core lib loading | `483e3a5ce` |
| Lazy TypeInterner (OnceLock) | ~5-10ms startup reduction | merged |
| Release profile (LTO, strip) | Binary 11MB → 5.2MB | merged |
| Generic instantiation cache | Fixed cache.clear() bug | `b1170fd5e` |
| Subtype relation caching | Cache (source,target)->bool | `b1170fd5e` |
| Export resolution caching | RwLock cache in binder | `b1170fd5e` |

---

## Remaining Losses (2 tests)

### 1. manyConstExports.ts — tsgo 2.40x faster

**Problem**: 222ms vs 92ms. Barrel file with 5000 const exports.

**Root Cause**: Even with caching, the initial resolution of 5000 exports is expensive. The cache helps on repeated lookups but not the first pass.

**Potential Fix**: Pre-index all exports during binding phase so resolution is O(1) from the start.

**Files**: `src/binder/state.rs`, `src/binder/state_binding.rs`

**Priority**: Medium - Only affects very large barrel files

---

### 2. 200 classes — tsgo 1.28x faster  

**Problem**: 110ms vs 86ms. Deep class hierarchy with 200 classes.

**Root Cause**: `get_class_instance_type` recomputes inheritance chain on every call.

**Potential Fix**: Add class instance type cache:
```rust
pub class_instance_type_cache: FxHashMap<(SymbolId, TypeListId), TypeId>
```

**Files**: `src/checker/class_type.rs`, `src/checker/context.rs`

**Priority**: Low - Only 1.28x slower, and tsz wins on 10/50/100 classes

---

## Low Priority / Future Work

### Type Interner Shard Tuning
- Current: 64 shards
- Could try 128/256 for high-core machines
- Diminishing returns - already fast

### SymbolIndex Integration  
- Use LSP's SymbolIndex for O(1) cross-file resolution
- ~400 LOC, architectural change
- Not needed given current performance

### Incremental Type Cache (LSP only)
- Salsa-style dependency tracking
- Only benefits LSP/watch mode
- ~600 LOC, high complexity

---

## Benchmark Script

Run comparisons with:
```bash
./scripts/bench-vs-tsgo.sh
```

---

## Changelog

| Date | Change |
|------|--------|
| Jan 31, 2026 | Initial plan with 8 optimizations |
| Jan 31, 2026 | Completed: lib loading, generic cache, subtype cache, export cache |
| Jan 31, 2026 | Results: tsz 29 vs tsgo 2 (was 28 vs 3) |
