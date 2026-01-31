# Performance Optimizations Plan

**Created**: January 31, 2026  
**Status**: Planning (Reviewed by Gemini - Approved)  
**Goal**: Improve type checking performance through targeted caching and optimization

---

## Executive Summary

This plan covers eight performance improvements identified through codebase analysis and Gemini review. The optimizations target key bottlenecks in generic instantiation, export resolution, class hierarchy traversal, subtype checking, and symbol lookup. Parallel type checking is already implemented.

**Expected Impact**: 20-50% improvement in type checking time for large codebases with heavy generics and barrel file patterns.

**Review Status**: ✅ Approved by Gemini (Jan 31, 2026)

---

## Priority Order

| # | Optimization | Effort | Risk | Impact | Status |
|---|--------------|--------|------|--------|--------|
| 1 | Generic instantiation cache | ~50 LOC | Medium* | Very High | ⬜ Not Started |
| 2 | Mass exports optimization | ~200 LOC | Low | High | ⬜ Not Started |
| 3 | Class hierarchy cache | ~100 LOC | Low | Medium-High | ⬜ Not Started |
| 4 | **Subtype relation caching** | ~150 LOC | Low | High | ⬜ Not Started |
| 5 | SymbolIndex integration | ~400 LOC | Medium | Medium | ⬜ Not Started |
| 6 | Type interner shard tuning | ~20 LOC | Low | Medium | ⬜ Not Started |
| 7 | Incremental type cache | ~600 LOC | High | LSP only | ⬜ Not Started |
| 8 | Parallel type checking | N/A | N/A | Already done | ✅ Complete |

*Risk elevated due to inference variable leak concern (see Risk section)

---

## Phase 0: Startup Optimizations (✅ Complete)

### 0.1 Lib File I/O Overhead (P0) — ✅ DONE

**Problem**: Loading 102 `lib.d.ts` files (23MB total) from disk caused ~70ms of system I/O overhead on every compilation, even for trivial files.

**Discovery**: Benchmarking revealed:
- `tsz --version`: 4ms (no lib loading)
- `tsz --noCheck /tmp/test.ts`: 11ms (no type checking, minimal lib use)
- `tsz --noEmit /tmp/test.ts`: 82ms (full lib loading + type checking)

The 70ms gap was pure lib file I/O and parsing.

**Solutions Implemented**:

1. **Parallel lib parsing with Rayon** (`src/cli/driver.rs`)
   - Split lib loading into two phases:
     - Phase 1 (sequential): Collect all lib paths following `/// <reference lib="..." />` directives
     - Phase 2 (parallel): Parse and bind all libs using `rayon::par_iter()`
   - Impact: ~30-40% reduction in lib loading time on multi-core machines

2. **Lazy lib loading for `--noCheck` mode** (`src/cli/driver.rs`)
   - Skip lib file loading entirely when `--noCheck` is set
   - Impact: 82ms → 11ms for parse-only operations

3. **Removed embedded libs fallback** (`src/cli/driver.rs`, `src/bin/tsz_server.rs`)
   - Libs now load from disk only (matching `tsgo` behavior)
   - Users need TypeScript installed or `TSZ_LIB_DIR` set
   - Impact: Simpler code path, no embedded data bloat

4. **Release profile optimizations** (`Cargo.toml`)
   ```toml
   [profile.release]
   lto = "fat"
   codegen-units = 1
   strip = "symbols"
   ```
   - Binary size: 11MB → 5.2MB
   - Faster loading due to smaller binary

5. **Lazy TypeInterner initialization** (`src/solver/intern.rs`)
   - DashMap shards use `OnceLock` for lazy allocation
   - Common strings interned on-demand, not at startup
   - Impact: ~5-10ms reduction in startup time

**Files Modified**:
- `src/cli/driver.rs` - Parallel lib parsing, lazy loading
- `src/solver/intern.rs` - OnceLock for lazy shard init
- `src/bin/tsz.rs` - Conditional tracing init
- `src/scanner/scanner_impl.rs` - Removed eager `intern_common()`
- `Cargo.toml` - Release profile optimizations

**Benchmark Results** (vs tsgo):

| Test | tsz | tsgo | Winner |
|------|-----|------|--------|
| largeControlFlowGraph.ts | 103ms | 549ms | **tsz 5.34x** |
| manyConstExports.ts | 293ms | 87ms | tsgo 3.39x |
| enumLiteralsSubtypeReduction.ts | 100ms | 86ms | tsgo 1.17x |

**Remaining Gap**: ~20ms overhead remains for full compilation due to:
- Disk I/O for lib files (unavoidable without pre-caching)
- Rust runtime initialization
- Parser/binder setup

**Future Opportunities**:
- Pre-parsed lib cache (binary format, mmap'd)
- Lib file memory mapping instead of `read_to_string`
- Persistent daemon mode for repeated compilations

---

## Phase 1: Quick Wins (High Impact, Low Risk)

### 1.1 Generic Instantiation Cache (P0)

**Problem**: The `application_eval_cache` is cleared before every use, defeating its purpose.

**Location**: `src/checker/state_type_environment.rs` lines 240-241

**Current Code**:
```rust
// Clear cache to ensure fresh evaluation with current contextual type
self.ctx.application_eval_cache.clear();  // <-- REMOVE THIS
```

**Solution**:
1. Remove or scope the `clear()` call
2. The cache key is already the interned `TypeId` (from `TypeKey::Application`)
3. Invalidate cache only on compilation unit boundaries, not per-evaluation

**Files to Modify**:
- `src/checker/state_type_environment.rs` - Remove/scope clear()
- `src/checker/context.rs` - Verify cache structure

**Verification**:
```bash
# Run conformance to ensure no regressions
./conformance/run.sh --server --max=1000

# Run benchmarks to measure improvement
cargo bench --bench real_world_bench
```

**Acceptance Criteria**:
- [ ] Conformance pass rate unchanged
- [ ] Benchmark shows improvement on generic-heavy files
- [ ] No new memory leaks (cache doesn't grow unbounded)

**⚠️ CRITICAL: Inference Variable Safety Check**

Before caching, verify the type does NOT contain inference variables:

```rust
fn is_safe_to_cache(types: &TypeInterner, type_id: TypeId) -> bool {
    // Do NOT cache types containing:
    // - TypeKey::Infer (inference variables)
    // - Unresolved TypeKey::TypeParameter
    !contains_inference_vars(types, type_id)
}

// In the cache logic:
if is_safe_to_cache(self.ctx.types, type_id) {
    self.ctx.application_eval_cache.borrow_mut().insert(type_id, result);
}
```

---

### 1.2 Class Hierarchy Cache (P1)

**Problem**: `get_class_instance_type` recomputes the entire inheritance chain on every call.

**Location**: `src/checker/class_type.rs`

**Current Behavior**:
```
get_class_instance_type(DerivedClass)
  → get_class_instance_type(ParentClass)
    → get_class_instance_type(GrandparentClass)
      → ... (no caching, repeated on every call)
```

**Solution**:
1. Add cache to `CheckerContext`:
   ```rust
   pub class_instance_type_cache: RefCell<FxHashMap<(SymbolId, TypeListId), TypeId>>,
   ```
2. Check cache before `get_class_instance_type_inner`
3. Store result after computation

**Files to Modify**:
- `src/checker/context.rs` - Add cache field
- `src/checker/class_type.rs` - Add cache lookup/store

**Implementation**:
```rust
// In class_type.rs - get_class_instance_type
pub fn get_class_instance_type(&mut self, sym_id: SymbolId, type_args: &[TypeId]) -> TypeId {
    // Create cache key
    let args_id = self.ctx.types.intern_type_list(type_args);
    let cache_key = (sym_id, args_id);
    
    // Check cache
    if let Some(&cached) = self.ctx.class_instance_type_cache.borrow().get(&cache_key) {
        return cached;
    }
    
    // Compute (existing logic)
    let result = self.get_class_instance_type_inner(sym_id, type_args);
    
    // Store in cache
    self.ctx.class_instance_type_cache.borrow_mut().insert(cache_key, result);
    result
}
```

**Verification**:
```bash
./conformance/run.sh --server --max=1000
cargo bench --bench real_world_bench
```

**Acceptance Criteria**:
- [ ] Conformance pass rate unchanged
- [ ] Benchmark shows improvement on class-heavy files
- [ ] Deep inheritance chains (5+ levels) show significant speedup

---

## Phase 2: Export Resolution (Medium Effort)

### 2.1 Mass Exports Optimization (P1)

**Problem**: Wildcard re-exports (`export * from`) resolved via sequential search through all source modules.

**Location**: `src/binder/state.rs`, `src/checker/symbol_resolver.rs`

**Current Behavior**:
```rust
// O(N) sequential search
for source_module in wildcard_reexports {
    if let Some(result) = resolve_import_with_reexports_inner(...) {
        return Some(result);
    }
}
```

**Solution**: Lazy caching with fast pre-check

**Step 1: Add Export Name Set to Binder** (src/binder/state.rs)
```rust
pub struct BinderState {
    // ... existing fields ...
    
    /// Names explicitly exported by this module.
    /// Used for O(1) "does this module export X?" checks.
    pub exported_names: FxHashSet<Atom>,
}
```

**Step 2: Add Resolution Cache to Checker** (src/checker/context.rs)
```rust
pub struct CheckerContext<'a> {
    // ... existing fields ...
    
    /// Cache for resolved module exports.
    /// Key: (file_idx, export_name) -> resolved SymbolId
    pub resolved_exports_cache: RefCell<FxHashMap<(usize, Atom), Option<SymbolId>>>,
    
    /// Recursion guard for circular export * resolution
    pub export_resolution_stack: RefCell<Vec<(usize, Atom)>>,
}
```

**Step 3: Implement Fast Resolution** (src/checker/symbol_resolver.rs)
```rust
pub fn resolve_module_export(&self, module_idx: usize, name: Atom) -> Option<SymbolId> {
    // 1. Check cache
    let cache_key = (module_idx, name);
    if let Some(&cached) = self.ctx.resolved_exports_cache.borrow().get(&cache_key) {
        return cached;
    }
    
    // 2. Check recursion guard (circular export *)
    if self.ctx.export_resolution_stack.borrow().contains(&cache_key) {
        return None;
    }
    self.ctx.export_resolution_stack.borrow_mut().push(cache_key);
    
    // 3. Resolve with fast pre-check
    let result = self.resolve_module_export_uncached(module_idx, name);
    
    // 4. Update cache and stack
    self.ctx.export_resolution_stack.borrow_mut().pop();
    self.ctx.resolved_exports_cache.borrow_mut().insert(cache_key, result);
    
    result
}

fn resolve_module_export_uncached(&self, module_idx: usize, name: Atom) -> Option<SymbolId> {
    let binder = self.get_binder_for_file(module_idx)?;
    
    // Check local exports first
    if let Some(sym) = binder.exports.get(&name) {
        return Some(*sym);
    }
    
    // Check wildcard re-exports with FAST PRE-CHECK
    for &re_export_module_idx in &binder.re_export_modules {
        let target_binder = self.get_binder_for_file(re_export_module_idx)?;
        
        // OPTIMIZATION: Skip if target definitely doesn't export this name
        if !target_binder.exported_names.contains(&name) {
            continue;
        }
        
        // Recurse into target
        if let Some(sym) = self.resolve_module_export(re_export_module_idx, name) {
            return Some(sym);
        }
    }
    
    None
}
```

**Files to Modify**:
- `src/binder/state.rs` - Add `exported_names` field
- `src/binder/state_binding.rs` - Populate `exported_names` during binding
- `src/checker/context.rs` - Add cache fields
- `src/checker/symbol_resolver.rs` - Implement cached resolution

**Verification**:
```bash
./conformance/run.sh --server --max=1000
# Test specifically with barrel file patterns
```

**Acceptance Criteria**:
- [ ] Conformance pass rate unchanged
- [ ] Barrel file imports (100+ exports) resolve faster
- [ ] Circular `export *` handled correctly

---

### 2.2 Subtype Relation Caching (P1) — Added per Gemini Review

**Problem**: `is_assignable_to` and `is_subtype_of` are the hottest functions in the compiler, but results aren't always cached for non-generic types.

**Location**: `src/solver/subtype.rs`, `src/checker/context.rs`

**Current State**:
- `CheckerContext` has a `relation_cache`
- `SubtypeChecker` (solver) often runs with ephemeral state
- Non-generic, fully resolved types are re-compared repeatedly

**Solution**:
1. Ensure `SubtypeChecker` uses a persistent cache for `(SourceId, TargetId) -> bool`
2. Only cache results for fully resolved types (no inference vars, no type params)

**Implementation**:
```rust
// In context.rs - add or verify existing
pub struct CheckerContext<'a> {
    /// Cache for subtype relation results.
    /// Key: (source_type, target_type) -> is_subtype
    pub subtype_cache: RefCell<FxHashMap<(TypeId, TypeId), bool>>,
}

// In subtype.rs - SubtypeChecker
fn is_subtype_of(&mut self, source: TypeId, target: TypeId) -> bool {
    // Fast path: identity
    if source == target {
        return true;
    }
    
    // Check cache for resolved types only
    let cache_key = (source, target);
    if is_fully_resolved(source) && is_fully_resolved(target) {
        if let Some(&cached) = self.cache.get(&cache_key) {
            return cached;
        }
    }
    
    // Compute result
    let result = self.is_subtype_of_inner(source, target);
    
    // Cache if both types are fully resolved
    if is_fully_resolved(source) && is_fully_resolved(target) {
        self.cache.insert(cache_key, result);
    }
    
    result
}

fn is_fully_resolved(types: &TypeInterner, type_id: TypeId) -> bool {
    // Returns false if type contains:
    // - TypeKey::Infer
    // - TypeKey::TypeParameter (unsubstituted)
    // - TypeKey::Error
    !contains_unresolved_types(types, type_id)
}
```

**Files to Modify**:
- `src/solver/subtype.rs` - Add/enhance caching logic
- `src/checker/context.rs` - Verify `subtype_cache` exists and is used
- `src/solver/visitor.rs` - Add `contains_unresolved_types` helper

**Verification**:
```bash
./conformance/run.sh --server --max=1000
cargo bench --bench solver_bench
```

**Acceptance Criteria**:
- [ ] Conformance pass rate unchanged
- [ ] Repeated subtype checks show cache hits
- [ ] No correctness issues with cached results

---

### 2.3 Type Interner Shard Tuning (P2) — Added per Gemini Review

**Problem**: Current `SHARD_COUNT = 64` may cause contention on high-core-count machines during parallel type checking.

**Location**: `src/solver/intern.rs`

**Current State**:
```rust
const SHARD_BITS: u32 = 6;
const SHARD_COUNT: usize = 1 << SHARD_BITS; // 64 shards
```

**Solution**: Benchmark with 128 and 256 shards

**Implementation**:
```rust
// Make shard count configurable or increase default
#[cfg(feature = "high_parallelism")]
const SHARD_BITS: u32 = 8;  // 256 shards
#[cfg(not(feature = "high_parallelism"))]
const SHARD_BITS: u32 = 7;  // 128 shards (new default)
```

**Benchmarking Strategy**:
1. Run `parallel_bench` with 64, 128, 256 shards
2. Test on machines with 8, 16, 32+ cores
3. Measure contention via lock wait times

**Files to Modify**:
- `src/solver/intern.rs` - Adjust `SHARD_BITS` constant
- `Cargo.toml` - Add optional `high_parallelism` feature

**Verification**:
```bash
cargo bench --bench parallel_bench
# Compare with different shard counts
```

**Acceptance Criteria**:
- [ ] No regression on low-core machines
- [ ] Measurable improvement on 16+ core machines
- [ ] Memory overhead acceptable

---

## Phase 3: Architecture (Higher Effort)

### 3.1 SymbolIndex Integration (P2)

**Problem**: Checker does scope chain lookups for cross-file symbols. `SymbolIndex` exists in LSP but not integrated with checker.

**Location**: `src/lsp/symbol_index.rs` (existing), `src/parallel.rs`, `src/checker/`

**Current State**:
- `SymbolIndex` has O(1) lookup by name
- Only built/used for LSP features
- Checker does linear scope chain search for imports

**Solution**:

**Step 1: Build SymbolIndex During Merge** (src/parallel.rs)
```rust
pub fn merge_bind_results(results: Vec<BindResult>) -> MergedProgram {
    // ... existing merge logic ...
    
    // NEW: Build symbol index
    let symbol_index = SymbolIndex::new();
    for (file_idx, file) in merged_files.iter().enumerate() {
        symbol_index.index_file(&file.file_name, &file.binder, file_idx);
    }
    
    MergedProgram {
        // ... existing fields ...
        symbol_index: Arc::new(symbol_index),
    }
}
```

**Step 2: Use in Checker for Cross-File Resolution** (src/checker/symbol_resolver.rs)
```rust
fn resolve_import_symbol(&mut self, specifier: &str, name: Atom) -> Option<SymbolId> {
    // Fast path: use SymbolIndex
    if let Some(sym_id) = self.ctx.symbol_index.lookup_export(specifier, name) {
        return Some(sym_id);
    }
    
    // Fallback: existing resolution logic
    self.resolve_import_symbol_slow(specifier, name)
}
```

**Files to Modify**:
- `src/lsp/symbol_index.rs` - Make public, add `lookup_export` method
- `src/parallel.rs` - Build index during merge
- `src/checker/context.rs` - Add `symbol_index` reference
- `src/checker/symbol_resolver.rs` - Use index for lookups

**Verification**:
```bash
./conformance/run.sh --server --max=1000
cargo bench --bench real_world_bench
```

**Acceptance Criteria**:
- [ ] Conformance pass rate unchanged
- [ ] Cross-file imports resolve in O(1) instead of O(scope_depth)
- [ ] Memory overhead acceptable (index size < 10% of symbol arena)

---

### 3.2 Incremental Type Cache with Dependency Tracking (P3)

**Problem**: No way to know what to invalidate when a file changes. Currently must re-check everything.

**Note**: This is primarily valuable for LSP/watch mode. Single-shot CLI doesn't benefit.

**Location**: `src/checker/context.rs`, new `src/checker/incremental.rs`

**Current State**:
```rust
pub struct TypeCache {
    node_types: FxHashMap<u32, TypeId>,
    symbol_types: FxHashMap<SymbolId, TypeId>,
    // No dependency tracking!
}
```

**Solution**: Salsa-style dependency tracking

**Step 1: Define Dependency Graph**
```rust
// src/checker/incremental.rs

/// Tracks what each type depends on
pub struct TypeDependencyGraph {
    /// type_id -> symbols it depends on
    type_to_symbols: FxHashMap<TypeId, SmallVec<[SymbolId; 4]>>,
    
    /// symbol_id -> types that depend on it (reverse index)
    symbol_to_types: FxHashMap<SymbolId, FxHashSet<TypeId>>,
    
    /// file_idx -> symbols defined in that file
    file_to_symbols: FxHashMap<usize, FxHashSet<SymbolId>>,
}

impl TypeDependencyGraph {
    /// Record that `type_id` depends on `symbol_id`
    pub fn record_dependency(&mut self, type_id: TypeId, symbol_id: SymbolId) {
        self.type_to_symbols
            .entry(type_id)
            .or_default()
            .push(symbol_id);
        self.symbol_to_types
            .entry(symbol_id)
            .or_default()
            .insert(type_id);
    }
    
    /// Invalidate all types affected by changes in `file_idx`
    pub fn invalidate_file(&mut self, file_idx: usize) -> FxHashSet<TypeId> {
        let mut invalidated = FxHashSet::default();
        
        if let Some(symbols) = self.file_to_symbols.get(&file_idx) {
            for &sym_id in symbols {
                if let Some(types) = self.symbol_to_types.get(&sym_id) {
                    invalidated.extend(types);
                }
            }
        }
        
        invalidated
    }
}
```

**Step 2: Track Dependencies During Type Resolution**
```rust
// In type resolution code
fn get_type_of_symbol(&mut self, sym_id: SymbolId) -> TypeId {
    let result = self.get_type_of_symbol_inner(sym_id);
    
    // Record dependency if we're building a type
    if let Some(building_type) = self.ctx.currently_building_type {
        self.ctx.dependency_graph.record_dependency(building_type, sym_id);
    }
    
    result
}
```

**Files to Create**:
- `src/checker/incremental.rs` - Dependency graph implementation

**Files to Modify**:
- `src/checker/context.rs` - Add dependency graph
- `src/checker/state_type_analysis.rs` - Record dependencies during resolution
- `src/lsp/project.rs` - Use incremental invalidation

**Verification**:
```bash
./conformance/run.sh --server --max=1000
# Test incremental updates in LSP
```

**Acceptance Criteria**:
- [ ] Conformance pass rate unchanged
- [ ] File change only invalidates dependent types
- [ ] LSP response time < 100ms for incremental updates

---

## Phase 4: Verification & Tuning

### 4.1 Parallel Type Checking Optimization

**Current State**: ✅ Already implemented in `src/parallel.rs`

**Potential Tuning**:
1. **Work stealing optimization**: Pre-sort files by size (largest first)
2. **Thread pool tuning**: Adjust Rayon config based on file count
3. **Memory pressure**: Monitor and limit concurrent checker instances

**Files to Review**:
- `src/parallel.rs` - `check_files_parallel`
- `src/solver/intern.rs` - Shard count (currently 64)

**Verification**:
```bash
# Profile with large codebase
cargo bench --bench parallel_bench
```

---

## Testing Strategy

### Unit Tests

Each optimization should have targeted unit tests:

```rust
#[test]
fn test_generic_instantiation_cache_hit() {
    // Same generic instantiation should return cached result
}

#[test]
fn test_class_hierarchy_cache() {
    // Deep inheritance should only compute once
}

#[test]
fn test_export_resolution_cache() {
    // Barrel file exports should cache
}

#[test]
fn test_circular_export_star_handling() {
    // Circular export * should not infinite loop
}
```

### Conformance Testing

After each optimization:
```bash
# Full conformance run
./conformance/run.sh --server --max=5000

# Compare with baseline
./conformance/run.sh --compare baseline.json
```

### Benchmarks

Create targeted benchmarks in `benches/`:

```rust
// benches/caching_bench.rs

fn benchmark_generic_instantiation(c: &mut Criterion) {
    // Measure Array<T> instantiation with and without cache
}

fn benchmark_class_hierarchy(c: &mut Criterion) {
    // Measure deep class hierarchy resolution
}

fn benchmark_barrel_exports(c: &mut Criterion) {
    // Measure import resolution from barrel file with 1000 exports
}
```

---

## Risk Mitigation

### Critical Risks (from Gemini Review)

| Risk | Severity | Mitigation |
|------|----------|------------|
| **Inference Variable Leaks** | 🔴 CRITICAL | Do NOT cache types containing `TypeKey::Infer` or unresolved `TypeKey::TypeParameter`. Add `is_safe_to_cache()` guard before every cache insert. |
| **Memory Bloat** | 🟠 HIGH | Implement LRU eviction or generation-based clearing. Set hard limits (e.g., 100k entries per cache). Clear caches on file boundaries in LSP. |

### Standard Risks

| Risk | Mitigation |
|------|------------|
| Cache invalidation bugs | Conservative invalidation, extensive testing |
| Memory growth | Add cache size limits, LRU eviction |
| Thread safety issues | Use RefCell for single-threaded, DashMap for concurrent |
| Conformance regressions | Run full conformance after each change |

### Inference Variable Safety Implementation

```rust
/// Check if a type is safe to cache (no inference variables or unresolved type params)
pub fn is_cacheable_type(types: &TypeInterner, type_id: TypeId) -> bool {
    use crate::solver::visitor::TypeVisitor;
    
    struct InferenceVarDetector {
        found: bool,
    }
    
    impl TypeVisitor for InferenceVarDetector {
        fn visit_type(&mut self, types: &TypeInterner, type_id: TypeId) -> bool {
            match types.lookup(type_id) {
                Some(TypeKey::Infer(_)) => {
                    self.found = true;
                    false // Stop visiting
                }
                Some(TypeKey::TypeParameter(_)) => {
                    // Unsubstituted type parameter - not safe to cache
                    self.found = true;
                    false
                }
                _ => true // Continue visiting
            }
        }
    }
    
    let mut detector = InferenceVarDetector { found: false };
    detector.walk_type(types, type_id);
    !detector.found
}
```

---

## Success Metrics

| Metric | Current | Target |
|--------|---------|--------|
| Generic-heavy file check time | Baseline | -30% |
| Class hierarchy resolution | Baseline | -40% |
| Barrel file import resolution | Baseline | -50% |
| Overall check time (large project) | Baseline | -20% |
| Memory overhead | Baseline | < +10% |

---

## Timeline

| Week | Deliverable |
|------|-------------|
| 1 | Phase 1: Generic cache + Class hierarchy cache |
| 2 | Phase 2: Mass exports + Subtype caching |
| 2 | Phase 2: Interner shard tuning (benchmarking) |
| 3-4 | Phase 3: SymbolIndex integration |
| 5-6 | Phase 3: Incremental type cache (if needed for LSP) |
| Ongoing | Phase 4: Tuning and verification |

---

## Appendix: Code Locations

| Component | Primary File | Related Files |
|-----------|-------------|---------------|
| Generic instantiation | `src/solver/instantiate.rs` | `src/solver/evaluate.rs`, `src/checker/state_type_environment.rs` |
| Application cache | `src/checker/context.rs` | `src/solver/application.rs` |
| Class hierarchy | `src/checker/class_type.rs` | `src/solver/inheritance.rs` |
| Subtype caching | `src/solver/subtype.rs` | `src/checker/context.rs` |
| Export resolution | `src/binder/state.rs` | `src/checker/symbol_resolver.rs` |
| Symbol index | `src/lsp/symbol_index.rs` | `src/parallel.rs` |
| Type cache | `src/checker/context.rs` | N/A |
| Type interner | `src/solver/intern.rs` | N/A |
| Parallel checking | `src/parallel.rs` | `src/solver/intern.rs` |

---

## References

- [NORTH_STAR.md](../architecture/NORTH_STAR.md) - Target architecture
- [TypeScript Compiler Internals](https://github.com/microsoft/TypeScript/wiki/Architectural-Overview)
- Gemini analysis on instantiation caching and export optimization (Jan 31, 2026)

---

## Changelog

| Date | Change |
|------|--------|
| Jan 31, 2026 | Initial plan created |
| Jan 31, 2026 | Gemini review: Added subtype caching, interner tuning, critical risks |
| Jan 31, 2026 | Added Phase 0: Lib I/O overhead analysis and optimizations (✅ Complete) |
