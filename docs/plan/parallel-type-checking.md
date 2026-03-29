# Multi-Threaded Type Checking for tsz

## Context

tsz currently type-checks files sequentially, even though parsing and binding are already parallelized with Rayon. The vs-tsgo benchmarks show tsz wins 64-0, but several benchmarks remain under 2x — and for real-world multi-file projects, sequential checking is the primary bottleneck.

tsgo uses **4 parallel type checkers**, each processing a file subset, achieving ~2-3x speedup from parallelism alone. TypeScript's `isolatedDeclarations` (TS 5.5+) enforces explicit type annotations on exports, enabling per-file independence — tsz already implements the full TS9xxx diagnostic suite for this.

**Goal**: Enable per-file parallel type checking using Rayon, matching tsgo's approach. This will make tsz dramatically faster on multi-file projects (the real-world use case) while maintaining correctness.

---

## Architecture Overview

```
BEFORE (current):
  Parse files (parallel) → Bind files (parallel) → Merge symbols → Check files (SEQUENTIAL) → Emit

AFTER (target):
  Parse files (parallel) → Bind files (parallel) → Merge symbols → Check files (PARALLEL) → Merge diagnostics → Emit
```

**Key principle**: Each file gets its own `CheckerState` with file-local mutable state. Cross-file data is either pre-computed and shared immutably (via `Arc`), or accessed through thread-safe structures (`DashMap`, `AtomicU32`).

---

## What's Already Thread-Safe (No Changes Needed)

| Component | Mechanism | File |
|-----------|-----------|------|
| `TypeInterner` | DashMap + thread-local cache | `crates/tsz-solver/src/intern/core.rs` |
| `DefinitionStore` | DashMap + AtomicU32 | `crates/tsz-solver/src/def/core.rs` |
| `NodeArena` (AST) | Immutable after parsing | `crates/tsz-parser/src/parser/node.rs` |
| `BinderState` | Immutable after binding, RwLock caches | `crates/tsz-binder/src/state/` |
| `all_arenas` / `all_binders` | `Arc<Vec<Arc<T>>>`, read-only | `crates/tsz-core/src/parallel/core.rs` |
| Global indices | `Arc<FxHashMap<...>>`, read-only | `crates/tsz-checker/src/context/core.rs` |
| Diagnostic collection | Per-file `Vec<Diagnostic>`, moved out | `crates/tsz-checker/src/context/mod.rs` |

---

## What Blocks Parallelism (43 RefCell/Cell fields in CheckerContext)

### Audit Summary

**File-local fields (33 fields — NO changes needed)**:
These are keyed by `NodeIndex` or `FlowNodeId` (per-file arena), or are per-file tracking state. Each parallel checker thread gets its own copy naturally.

- 7 flow analysis fields (worklist, visited, results, caches) — `FlowNodeId` keys
- 4 narrowing/reference caches — `(u32, u32)` positional keys
- 5 class caches — `NodeIndex` keys (class_chain_summary, heritage_symbol, base_constructor/instance)
- 1 JSDoc resolution state
- 1 typeof resolution stack
- 4 recursion depth counters (`Cell<u32>`)
- 4 depth counters (`RefCell<DepthCounter>`)
- 1 JSDoc anchor position (`Cell<u32>`)
- All diagnostics, dedup sets, deferred errors — per-file accumulation
- `checked_classes`, `checking_classes` — per-file `FxHashSet<NodeIndex>`
- All scope maps, resolution stacks — per-file

**Cross-file fields (10 fields — NEED changes)**:

| Line | Field | Type | Current | Target |
|------|-------|------|---------|--------|
| 1171 | `symbol_to_def` | `RefCell<FxHashMap<SymbolId, DefId>>` | Warmed from DefinitionStore, merged back | Pre-warm per-thread; write-back to `DefinitionStore` via `DashMap` |
| 1176 | `def_to_symbol` | `RefCell<FxHashMap<DefId, SymbolId>>` | Same | Same |
| 1182 | `def_type_params` | `RefCell<FxHashMap<DefId, Vec<TypeParamInfo>>>` | Accessed via Arc | Already in `DefinitionStore` (DashMap) — use directly |
| 1186 | `def_no_type_params` | `RefCell<FxHashSet<DefId>>` | Same | Same |
| 1212 | `cross_file_symbol_targets` | `RefCell<FxHashMap<SymbolId, usize>>` | Overlay + base | Pre-compute ALL targets in merge phase; store in `Arc<FxHashMap>` (read-only) |
| 846 | `class_symbol_to_decl_cache` | `RefCell<FxHashMap<SymbolId, Option<NodeIndex>>>` | Cross-file SymbolId keys | Keep per-thread (duplicate work acceptable) |
| 863 | `class_decl_miss_cache` | `RefCell<FxHashSet<TypeId>>` | Cross-file TypeId keys | Keep per-thread |
| 842 | `env_eval_cache` | `RefCell<FxHashMap<TypeId, EnvEvalCacheEntry>>` | Cross-file TypeId keys | Keep per-thread |
| 684 | `type_environment` | `Rc<RefCell<TypeEnvironment>>` | Shared via Rc (NOT thread-safe) | `Arc<RwLock<TypeEnvironment>>` or per-thread with lazy resolution |
| 1161 | `type_env` | `RefCell<TypeEnvironment>` | Cloned per context, merged back | Per-thread (no merge needed for parallel files) |

**QueryCache (per-thread, NOT shared)**:
- `crates/tsz-solver/src/caches/query_cache.rs` — 11 `RefCell<FxHashMap>` fields
- Already per-thread by design. Each Rayon worker creates its own `QueryCache::new(&interner)`.
- Duplicate computation across threads is acceptable (same as tsgo's approach).

---

## Implementation Steps

### Step 1: Make `cross_file_symbol_targets` fully pre-computed — DONE (ff23aa7a)

**Files**: `crates/tsz-core/src/parallel/core.rs`, `crates/tsz-checker/src/context/mod.rs`, `crates/tsz-checker/src/context/core.rs`

**Current**: `symbol_file_targets` is a `Vec<(SymbolId, usize)>` pre-computed in `check_files_parallel()` (line 3426-3435), then registered per-file into `cross_file_symbol_targets: RefCell<FxHashMap>`. During cross-file delegation, the overlay can be extended.

**Change**:
1. In `check_files_parallel()`, convert the pre-computed targets into an `Arc<FxHashMap<SymbolId, usize>>` (one allocation, shared read-only)
2. Store as `global_symbol_file_index: Option<Arc<FxHashMap<SymbolId, usize>>>` (this field already exists at line 1219!)
3. Make `resolve_symbol_file_index()` only read from the `Arc` map — remove the `RefCell` overlay
4. If cross-file delegation discovers new targets at runtime, insert into `DefinitionStore.symbol_def_index` (already DashMap, thread-safe)

**Verification**: Run `cargo test --package tsz-checker --lib` and full conformance.

### Step 2: Convert `symbol_to_def` / `def_to_symbol` to use DefinitionStore directly — DONE (8a1a4497)

**Files**: `crates/tsz-checker/src/context/mod.rs`, `crates/tsz-checker/src/state/type_analysis/cross_file.rs`, `crates/tsz-checker/src/state/type_environment/def_mapping.rs`

**Current**: Each checker pre-warms local `RefCell<FxHashMap>` caches from `Arc<DefinitionStore>` via `warm_local_caches_from_shared_store()`. Cross-file delegation merges back from child to parent.

**Change**:
1. Keep the local `RefCell` caches as thread-local read caches (fast path)
2. On cache miss, fall through to `DefinitionStore` (DashMap, thread-safe)
3. On new DefId creation during checking, write to both local cache AND `DefinitionStore`
4. Remove the merge-back step in `delegate_cross_arena_symbol_resolution()` — the `DefinitionStore` is the authoritative store
5. `def_type_params` and `def_no_type_params` already exist in `DefinitionStore` — route lookups there directly

**Key method to modify**: `delegate_cross_arena_symbol_resolution()` in `cross_file.rs` (lines 83-450). Currently creates a child checker, delegates, then merges `def_to_symbol`, `symbol_to_def`, `def_type_params`, and `type_env` back. With thread-safe DefinitionStore, the child writes directly to the shared store, eliminating the merge.

**Verification**: Run cross-file conformance tests, multi-file test projects.

### Step 3: Convert `type_environment` from `Rc<RefCell>` to per-thread — DONE (770dadc8)

**Files**: `crates/tsz-checker/src/context/mod.rs`, `crates/tsz-checker/src/context/constructors.rs`, `crates/tsz-checker/src/state/type_environment/core.rs`

**Current**: `Rc<RefCell<TypeEnvironment>>` shared between parent and child checkers for cross-file delegation.

**Change**:
1. Each file's checker builds its own `TypeEnvironment` during `build_type_environment()` (already happens)
2. For cross-file delegation, the child checker builds a fresh `TypeEnvironment` from the target file's binder
3. Results (DefId → TypeId mappings) are written to `DefinitionStore` (thread-safe), not merged back via `Rc`
4. Replace `Rc<RefCell<TypeEnvironment>>` with plain `TypeEnvironment` (owned, non-shared)
5. The `type_env: RefCell<TypeEnvironment>` field becomes the sole type environment (no more Rc sharing)

**Critical consideration**: `TypeEnvironment` contains `DefId → TypeId` mappings built lazily during checking. When file B imports from file A, B's checker needs A's exported types. With parallel checking:
- Option A: Pre-compute export signatures in a sequential pass (safest)
- Option B: Query `DefinitionStore` which lazily delegates to the owning file's checker (more complex)
- **Recommended**: Option A for initial implementation — it's simpler and what tsgo does

**Verification**: Multi-file test projects with cross-file imports.

### Step 4: Make speculation thread-compatible

**Files**: `crates/tsz-checker/src/context/speculation.rs`

**Current**: Speculation snapshots/restores `diagnostics`, `node_types`, `flow_analysis_cache`, and other per-file state using `Vec::truncate()` and `FxHashSet::clone()`.

**Change**: No changes needed! Speculation is entirely per-file:
- `DiagnosticSnapshot` — per-file `Vec<Diagnostic>`
- `FullSnapshot` — per-file dedup sets
- `CacheSnapshot` — per-file `NodeTypeCache` and `flow_analysis_cache`

Since each parallel checker thread owns its own `CheckerState`, speculation is naturally thread-local. No synchronization needed.

### Step 5: Enable Rayon parallel dispatch in `check_files_parallel()` — DONE (1629452d)

**File**: `crates/tsz-core/src/parallel/core.rs` (lines 3397-3496)

**Current**: Uses `maybe_parallel_iter!()` macro but effectively runs sequentially because of shared mutable state.

**Change**:
```rust
// In check_files_parallel(), after Steps 1-4 are done:

let global_targets = Arc::new(symbol_file_targets.into_iter().collect::<FxHashMap<_, _>>());

let file_results: Vec<FileCheckResult> = program.files
    .par_iter()  // Rayon parallel iterator
    .enumerate()
    .map(|(file_idx, file)| {
        // Per-thread QueryCache (already per-thread)
        let query_cache = QueryCache::new(&program.type_interner);

        // Per-file CheckerState
        let mut checker = CheckerState::new_with_shared_def_store(
            &file.arena,
            &shared_binders[file_idx],
            &query_cache,
            file.file_name.clone(),
            &checker_options,
            Arc::clone(&program.definition_store),
        );

        // Set shared read-only context
        checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
        checker.ctx.set_all_binders(Arc::clone(&all_binders));
        checker.ctx.set_global_symbol_file_index(Arc::clone(&global_targets));
        checker.ctx.set_current_file_idx(file_idx);

        if !lib_contexts.is_empty() {
            checker.ctx.set_lib_contexts(lib_contexts.clone());
        }

        // Check the file
        checker.check_source_file(root_idx);

        // Move diagnostics out (owned, no sharing)
        FileCheckResult {
            file_idx,
            file_name: file.file_name.clone(),
            diagnostics: std::mem::take(&mut checker.ctx.diagnostics),
        }
    })
    .collect();
```

**File sorting**: Sort files largest-first before dispatch so Rayon's work-stealing starts expensive files early (already done in current code).

**Verification**: Full benchmark suite, conformance suite, multi-file test projects.

### Step 6: Diagnostic merging and deduplication — DONE (included in Step 5)

**File**: `crates/tsz-core/src/parallel/core.rs`

**Current**: Diagnostics are collected per-file and returned as `Vec<FileCheckResult>`.

**Change**:
1. After parallel checking, merge all `Vec<Diagnostic>` from each file
2. Sort by `(file_index, start_position)` for deterministic output
3. Deduplicate by `(file_index, start_position, error_code)` — same diagnostic from different checkers
4. This matches tsgo's approach: check in parallel, merge+dedup after

```rust
let mut all_diagnostics: Vec<Diagnostic> = file_results
    .into_iter()
    .flat_map(|r| r.diagnostics)
    .collect();
all_diagnostics.sort_by_key(|d| (d.file_index, d.start));
all_diagnostics.dedup_by(|a, b| a.start == b.start && a.code == b.code && a.file_index == b.file_index);
```

**Verification**: Compare diagnostic output of parallel vs sequential runs on test projects.

### Step 7: Cross-file delegation under parallelism

**File**: `crates/tsz-checker/src/state/type_analysis/cross_file.rs`

**Current**: `delegate_cross_arena_symbol_resolution()` creates a child `CheckerState` pointing at the target file's arena, runs type resolution, and merges results back to parent.

**Change for parallel mode**:
1. When file B needs a type from file A, B's checker calls `delegate_cross_arena_symbol_resolution()` as before
2. The child checker uses file A's arena and binder (accessed via `all_arenas[file_idx]` and `all_binders[file_idx]`)
3. Results are written to `DefinitionStore` (thread-safe) instead of merged back to parent's RefCell
4. B's checker reads from `DefinitionStore` to get the resolved type
5. No merge-back of `def_to_symbol`, `symbol_to_def`, `type_env` — all go through the shared store

**Thread safety of delegation**: The child checker runs on the same thread as the parent (not spawned to another thread). This means:
- The child's arena/binder reads are safe (immutable data via Arc)
- The child writes to DefinitionStore (DashMap, thread-safe)
- No RefCell sharing between parent and child (child gets fresh RefCells)
- The `CROSS_ARENA_DEPTH` thread-local prevents infinite recursion (already exists)

**Verification**: Multi-file projects with circular imports, re-exports, namespace augmentation.

### Step 8: `isolatedDeclarations` fast path (future optimization)

**Files**: `crates/tsz-checker/src/state/state_checking/isolated_declarations.rs`, `crates/tsz-core/src/parallel/core.rs`

When `isolatedDeclarations: true`:
1. Before parallel checking, generate `.d.ts` signatures for all files via syntax stripping (parallel, no type inference)
2. Each file's checker reads import types from pre-computed `.d.ts` signatures instead of delegating to the source file's checker
3. This eliminates cross-file delegation entirely — maximum parallelism
4. tsz already has the full TS9xxx diagnostic suite (1,363 lines in `isolated_declarations.rs`)

**This step is deferred** — Steps 1-7 provide the foundation. The `isolatedDeclarations` fast path is a further optimization on top.

---

## Files to Modify

| File | Changes |
|------|---------|
| `crates/tsz-checker/src/context/mod.rs` | Remove `cross_file_symbol_targets` RefCell overlay; convert `type_environment` from `Rc<RefCell>` to owned |
| `crates/tsz-checker/src/context/core.rs` | Update `resolve_symbol_file_index()` to use only `Arc` map; remove overlay mutation |
| `crates/tsz-checker/src/context/constructors.rs` | Update `with_parent_cache()` — no more Rc clone for type_environment; route def lookups to DefinitionStore |
| `crates/tsz-checker/src/state/type_analysis/cross_file.rs` | Update `delegate_cross_arena_symbol_resolution()` — write to DefinitionStore instead of merge-back |
| `crates/tsz-checker/src/state/type_environment/def_mapping.rs` | Route `symbol_to_def`/`def_to_symbol` cache misses to DefinitionStore |
| `crates/tsz-checker/src/state/type_environment/core.rs` | `build_type_environment()` — write results to DefinitionStore |
| `crates/tsz-core/src/parallel/core.rs` | Convert `check_files_parallel()` to true Rayon parallel dispatch; add diagnostic merging |
| `crates/tsz-solver/src/def/core.rs` | Add missing `DashMap` methods to DefinitionStore for write-back from checkers |

---

## Verification Plan

### Unit Tests
```bash
cargo test --package tsz-checker --lib
cargo test --package tsz-solver --lib
cargo test --package tsz-core --lib
```

### Conformance (no regressions)
```bash
scripts/safe-run.sh ./scripts/conformance/conformance.sh run
# Must match or exceed current snapshot (11394+)
```

### Multi-File Test Projects
```bash
# Run existing multi-file integration tests
cargo test --package tsz-core --test parallel_tests

# Create test with cross-file imports:
# file_a.ts: export interface Foo { x: number }
# file_b.ts: import { Foo } from './file_a'; const f: Foo = { x: 1 };
# Verify same diagnostics in parallel vs sequential mode
```

### Benchmark Validation
```bash
# Single-file benchmarks should NOT regress
./scripts/bench/bench-vs-tsgo.sh --quick

# Multi-file benchmark (new):
# Create 100-file project, measure parallel vs sequential
time .target-bench/dist/tsz --noEmit tsconfig.json  # parallel
TSZ_SEQUENTIAL=1 time .target-bench/dist/tsz --noEmit tsconfig.json  # sequential fallback
```

### Determinism Check
```bash
# Run parallel checking 10 times, verify identical diagnostic output
for i in $(seq 1 10); do
  .target-bench/dist/tsz --noEmit tsconfig.json 2>&1 | md5
done
# All hashes must match
```

### Thread Safety (CI)
```bash
# Run under ThreadSanitizer (if available)
RUSTFLAGS="-Z sanitizer=thread" cargo test --package tsz-core --test parallel_tests
```

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Cross-file delegation causes data races | Child checker runs on SAME thread as parent; DefinitionStore uses DashMap |
| Diagnostic ordering differs from sequential | Sort by (file_index, start_position) after parallel collection |
| Duplicate diagnostics from multiple checkers | Dedup by (file_index, start, code) — same as tsgo |
| Performance regression on single-file | Per-file parallelism has zero overhead for single files (Rayon short-circuits) |
| Memory increase from duplicate caches | ~20% increase (matches tsgo); QueryCache is lightweight per-thread |
| Speculation conflicts across threads | Speculation is per-file, never crosses thread boundaries |
| `Rc<RefCell<TypeEnvironment>>` is !Send | Convert to owned or `Arc<RwLock>` in Step 3 |

---

## Implementation Order & Dependencies

```
Step 1 (cross_file_symbol_targets) ─────┐
Step 2 (symbol_to_def via DefinitionStore) ──┤
Step 3 (type_environment per-thread) ────┤── All independent, can be done in parallel
Step 4 (speculation — no changes needed) ┘
                                         │
                                         ▼
Step 5 (Rayon parallel dispatch) ────────── Depends on Steps 1-3
                                         │
                                         ▼
Step 6 (diagnostic merging) ─────────────── Depends on Step 5
                                         │
                                         ▼
Step 7 (cross-file delegation) ──────────── Depends on Steps 2, 5
                                         │
                                         ▼
Step 8 (isolatedDeclarations fast path) ── Future, depends on Step 7
```

Steps 1-3 can be implemented and merged independently (each is a safe refactor that works in single-threaded mode). Step 5 is the "flip the switch" moment that enables actual parallelism.
