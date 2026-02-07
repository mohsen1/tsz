# Thread Safety & Parallel Checking Roadmap

**Date**: 2026-02-07
**Status**: Level 1 done, Levels 2-3 planned

---

## Current State

The TypeInterner (64-shard DashMap) and QueryCache (RwLock) are already thread-safe.
CheckerState/CheckerContext are NOT thread-safe (50+ mutable fields, RefCell, FxHashMap).

However, each file creates its **own** CheckerState, so file-level parallelism works
without any CheckerState changes. Level 1 (below) is implemented.

---

## Level 1: File-Level Parallelism — DONE

Non-cached CLI builds (first build, CI) now check all files in parallel using rayon.
Each file gets its own CheckerState; shared state (TypeInterner, QueryCache) is thread-safe.

Cached builds (watch mode) still use the sequential work queue with export-hash dependency
cascade, since the cascade is inherently sequential.

Implementation: `check_file_for_parallel()` in `src/cli/driver.rs`, called from the
parallel branch of `collect_diagnostics()`.

---

## Level 2: Async LSP + Request Cancellation

**Goal**: Responsive LSP during heavy checking — hover/completion shouldn't block on diagnostics.

**Effort**: ~1-2 weeks

### Problem

The LSP server (`src/bin/tsz_lsp.rs`) is single-threaded and synchronous:
- Blocking `loop {}` reading stdin (line 830)
- Each request handled synchronously via `handle_message()`
- While one file is being checked, all other requests wait

### Approach

1. Make the LSP server async (tokio runtime):
   - Main task: read JSON-RPC from stdin, dispatch requests
   - Background tasks: diagnostics computation, heavy features (completions, references)

2. Background diagnostics:
   - On `didChange`: debounce, then spawn diagnostics computation on a thread pool
   - Publish results via `textDocument/publishDiagnostics` notification
   - If another `didChange` arrives before diagnostics finish, cancel the in-progress task

3. Request cancellation (`$/cancelRequest`):
   - Track in-flight requests
   - When a cancel arrives, abort the associated task
   - Particularly important for completions (typing fast = many cancellations)

4. Prioritization:
   - Interactive requests (hover, completion, definition) have higher priority
   - Background diagnostics are low priority and cancellable

### Key Design Decisions

- CheckerState stays `&mut self` — no changes needed
- Each diagnostics task creates its own CheckerState
- Use `tokio::sync::watch` or `CancellationToken` for cooperative cancellation
- The `Project` struct needs to become `Arc<RwLock<Project>>` or split into
  immutable (files, binders) and mutable (caches, diagnostics) parts

### Files to Change

- `src/bin/tsz_lsp.rs` — async event loop, request dispatch
- `crates/tsz-lsp/src/project.rs` — split Project for concurrent access
- New: cancellation token plumbing through CheckerState

---

## Level 3: Intra-File Parallelism (Declaration-Level)

**Goal**: Faster checking of large files (5K+ lines) by checking declarations in parallel.

**Effort**: ~2-4 weeks

### Problem

`CheckerContext` has ~50 mutable fields that prevent concurrent access:

**Caches (20+ fields)**:
- `symbol_types: FxHashMap<SymbolId, TypeId>`
- `node_types: FxHashMap<u32, TypeId>`
- `application_eval_cache`, `mapped_eval_cache`, `element_access_type_cache`, etc.

**Recursion guards (10+ fields)**:
- `symbol_resolution_stack: Vec<SymbolId>`
- `node_resolution_set: FxHashSet<NodeIndex>`
- `class_instance_resolution_set`, `class_constructor_resolution_set`, etc.

**Output (3 fields)**:
- `diagnostics: Vec<Diagnostic>`
- `emitted_diagnostics: FxHashSet<(u32, u32)>`

**Scoped state (10+ fields)**:
- `return_type_stack: Vec<TypeId>`
- `this_type_stack: Vec<TypeId>`
- `type_parameter_scope: FxHashMap<String, TypeId>`

### Options

#### Option A: Mechanical Lock Wrapping

Replace `FxHashMap` with `DashMap`, `RefCell` with `RwLock`, `Cell` with `Atomic`.

- Pros: Minimal structural change
- Cons: Lock overhead on every cache lookup (node_types is hit per-node).
  Could be 2-5x slower on single-threaded workloads.
  Not recommended for hot-path caches.

#### Option B: Clone-per-Declaration, Merge Results

For each top-level declaration, clone a lightweight "declaration context" from the
file context. Check independently. Merge diagnostics at the end.

- Pros: No locking in hot path
- Cons: Cache misses increase (no cross-declaration sharing), merge logic is complex
- Best for: CPU-bound checking where declarations are independent

#### Option C: Freeze/Snapshot Pattern (Recommended, rust-analyzer style)

Build `TypeEnvironment` and file-level setup in a single-threaded "write" phase.
Freeze into an immutable `Arc<FileContext>`. Per-declaration checks get their own
mutable `DeclContext` but share the frozen file context via Arc.

```
File-Level Setup (single-threaded):
  1. Parse pragmas
  2. Build type environment
  3. Register boxed types
  4. Build flow graph
  → Freeze into Arc<FileContext>

Per-Declaration Check (parallel):
  DeclContext {
      shared: Arc<FileContext>,     // immutable, zero-cost sharing
      local_cache: FxHashMap<...>,  // mutable, per-declaration
      diagnostics: Vec<Diagnostic>, // mutable, per-declaration
      recursion_guard: ...,         // mutable, per-declaration
  }
```

- Pros: Clean architecture, immutable shared state has zero synchronization cost
- Cons: Requires splitting CheckerContext into "file-level immutable" and
  "decl-level mutable" parts — significant refactoring

### Prerequisites for Level 3

1. **Phase 1.5 from checker splitting** (identified in salsa-incremental-lsp.md):
   - Make `check_statement` return `Vec<Diagnostic>` instead of push-based
   - Split `TypeEnvironment` from eager to lazy (on-demand per-declaration)
   - Extract global checks into standalone file-level passes

2. **5 mandatory setup steps** must run before any declaration check:
   - Pragma parsing
   - Cache clearing
   - `build_type_environment()` — O(file_size), can't skip
   - Flow analysis setup
   - `register_boxed_types()`

3. **Cross-declaration references**: Function A calls function B → need B's type.
   Requires `decl_type()` queries that can be resolved from the frozen file context.

### When to Do This

Only if benchmarks show that single-file checking is the bottleneck for LSP
responsiveness. Current data (2026-02-07):
- 5K line file: ~8.6ms full pipeline
- 2.6K line file: ~4.6ms
- Most real-world files check in under 10ms

Level 3 only helps for very large files (10K+ lines) that take >50ms.
For most projects, Level 1 (file-level parallelism) is sufficient.
