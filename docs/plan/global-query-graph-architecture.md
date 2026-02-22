# RFC: Zero-Config Monorepo Parallelism (The Global Query Graph)

**Status**: Draft  
**Owner**: TSZ team  
**Date**: February 22, 2026  
**Audience**: Compiler and LSP maintainers  

## 1. Executive Summary

Historically, scaling TypeScript in large monorepos required manual orchestration using "Project References" (`tsconfig.json` `references`). This forces developers into a rigid, acyclic, and manually maintained build DAG. It creates pipeline stalls, memory spikes, and forces architectural compromises (e.g., merging packages to avoid cross-project circular dependencies).

This RFC proposes a radical departure from the `tsc --build` orchestration model. `tsz` will abandon "Projects" as the unit of concurrency and instead adopt a **Demand-Driven Global Symbol Graph**. 

**The Killer Feature:** `tsz` will advertise that developers can **delete their project references**. `tsz` will automatically discover the dependency graph, parallelize work at the symbol level across the entire monorepo, and—unlike `tsc`—safely allow and resolve circular dependencies between packages.

---

## 2. Core Architecture: The Two-Brain Model

To achieve massive parallelism while remaining strictly `tsc` compatible for legacy users, `tsz` splits orchestration into two layers:

### 2.1 The Compatibility Frontend (The Facade)
For users who keep their `tsconfig.json` project references (e.g., for discrete `.js`/`.d.ts` emitting), the Frontend reads the DAG and validates it. It can emit `TS6202` (circular reference) if strict compatibility is requested. However, it does *not* use this DAG for execution.

### 2.2 The Query Backend (The Engine)
The core compiler ignores artificial project boundaries. It treats the entire monorepo as a single fluid sea of files. It resolves types using a demand-driven query engine (inspired by Rust's Salsa). 

---

## 3. The Phased Demand-Driven Pipeline

To handle TypeScript's unique semantics (Declaration Merging and Return Type Inference) without deadlocks or memory corruption, `tsz` uses a phased query architecture:

### Phase 1: Global Indexing (The "Skeleton" Pass)
TypeScript requires global knowledge of ambient declarations before any type resolution can occur.
1. **Concurrent Fast-Parse:** All files are parsed in parallel.
2. **Stable Identification:** We do *not* use `NodeIndex` for global symbols. We extract a "Skeleton" of the file, mapping exports, interfaces, and function signatures to stable `DefId`s (Definition IDs) and byte spans.
3. **Global Merge:** We merge global augmentations (e.g., `interface Window`) and module graphs sequentially. Because we only merge the *Skeleton* (dropping the ASTs immediately), this takes milliseconds and avoids Amdahl's Law bottlenecks.

### Phase 2: Demand-Driven Resolution (The Salsa Engine)
We transition to a concurrent query engine (`QueryDatabase`). There is no artificial split between "API" and "Body" checking; everything is a query.
1. **Query Execution:** A worker thread asks for the type of a `DefId`. 
2. **On-Demand AST Hydration:** If the query requires checking a function body, the Virtual File System reads the file, re-parses it, and maps the stable `DefId` back to the temporary `NodeIndex` for local evaluation. The AST is held in a short-lived thread-local Arena and dropped immediately after the query completes.
3. **Cycle-Aware Wait Graph:** If Thread A's query depends on Thread B's query, it registers an edge in a global Wait-Graph. If a multi-thread cyclic deadlock is detected, the engine panics the participating threads, rolls back their state, and executes the Strongly Connected Component (SCC) on a single thread using Fixpoint Iteration.

---

## 4. Solving the Memory Crisis: Stable IDs vs. Arenas

The legacy `tsc` model keeps all ASTs in memory. Previous `tsz` iterations tightly coupled `Symbol`s to `NodeIndex`es, making it impossible to drop ASTs without creating dangling pointers.

**The Fix: The DefId Indirection**
*   **The Global Tier:** The `SymbolArena` and `TypeInterner` never store `NodeIndex`es. They only store `DefId`s, which represent a stable logical path (e.g., `FileId(42) + DeclarationPath("export class Foo")`).
*   **The Local Tier:** Worker threads instantiate short-lived `NodeArena`s to answer queries. They map the `DefId` to a local `NodeIndex`, compute the type, intern the result into the Global Tier, and then completely destroy the `NodeArena`.
*   **Result:** Peak memory (RSS) is mathematically bounded by `(Global_Symbol_Graph_Size) + (Worker_Thread_Count * Average_File_AST_Size)`. It will never OOM, regardless of monorepo size.

---

## 5. Incremental Caching & `.tsbuildinfo`

We replace coarse `tsc` cache invalidation with **Semantic API Fingerprinting**.

1. When a file is modified, we fast-parse it to generate its Outline.
2. If the Outline (the API Fingerprint) matches the cached Outline, we **do not** invalidate downstream files, even if they are in different packages.
3. We only drop to Phase 4 (Body Checking) for the specific file that changed.
4. We synthesize fake `.tsbuildinfo` files on disk purely to satisfy external tools or legacy scripts, while keeping the true incremental graph in a centralized `tsz` cache.

---

## 5. Concrete Action Items (Refactoring Path)

To transition the existing codebase to this architecture, the following critical refactoring steps are required:

### 5.1 Refactor `Symbol` to Remove `NodeIndex` (The Memory Fix)
Currently, `Symbol` in `crates/tsz-binder/src/lib.rs` tightly couples to `NodeIndex` (e.g., `pub declarations: Vec<NodeIndex>`). This prevents dropping the AST (`NodeArena`).
*   **Action:** Change `Symbol` to store stable logical pointers—either a `DefId` mapped to a File ID + Byte Span (`[start, end]`), or a lightweight "Skeleton IR". 
*   **Goal:** When a worker thread needs to re-hydrate an AST to evaluate a body, it parses the file and uses the Byte Span to re-discover the correct `NodeIndex` in the new, short-lived `NodeArena`.

### 5.2 Gut `MergedProgram` and Sequential Merging (The Amdahl Fix)
Currently, `src/parallel.rs` parses files concurrently but then calls `merge_bind_results`, which sequentially merges every file's `SymbolArena` into a single `MergedProgram` and retains every `Arc<NodeArena>`.
*   **Action:** Delete the sequential merge phase. `parse_and_bind_parallel` should extract the "Skeletons" (exported symbols and ambient declarations) and insert them concurrently into a lock-free `DashMap` (already available in `Cargo.toml`).
*   **Goal:** Immediately drop the `NodeArena`s after extracting the Skeleton, keeping global memory flat and initialization time near-zero.

### 5.3 Invert CLI Control Flow (The Execution Fix)
Currently, `crates/tsz-cli/src/build.rs` (`build_solution`) orchestrates builds by topologically sorting `tsconfig.json` references and compiling projects in a sequential loop.
*   **Action:** The CLI must stop managing the build graph. It should simply initialize the `QueryDatabase` with the workspace root and invoke a single, top-level query: `db.check_workspace()`.
*   **Goal:** The `QueryDatabase`'s demand-driven engine takes over, spawning parallel worker threads that traverse the global import graph, hydrating ASTs on-demand, and resolving types concurrently.

### 5.4 Deterministic Diagnostics Emitting (The Output Fix)
When `db.check_workspace()` executes concurrently across the entire monorepo, worker threads will generate diagnostics in a non-deterministic order based on OS thread scheduling. 
*   **Action:** The `QueryDatabase` must buffer all generated diagnostics internally during execution rather than streaming them directly to the console.
*   **Goal:** Before the CLI exits, it must collect the buffered diagnostics, sort them deterministically (e.g., by file path, then by line/column number), and print them sequentially. This ensures that `tsz` output remains perfectly stable and predictable across identical CI runs, matching `tsc`'s sequential emission style.

---

## 6. Marketing & Adoption Strategy

We will explicitly advertise this feature as a reason to migrate to `tsz`:
> *"Tired of Project Reference Hell? Delete your `references` arrays. `tsz` builds a global symbol graph in milliseconds, parallelizes your entire monorepo automatically, and even lets you keep those circular dependencies."*

---

## 7. Testing & Validation Strategy

To ensure absolute `tsc` compatibility and performance regressions aren't introduced, the following validation framework must be established:

### 7.1 Correctness & Compatibility
*   **Conformance Test Harness Extension:** The `tests/conformance/` suite must be run with a new `--global-graph` flag that simulates dissolving multi-project `tsconfig.json` test fixtures into a single compilation context.
*   **Cyclic Resolution Assertions:** Create synthetic test graphs in `crates/tsz-checker/src/tests/` with cross-package circular dependencies (e.g., `pkg_a` exporting a class extending an interface from `pkg_b`, which imports an enum from `pkg_a`). Assert that `tsz` correctly evaluates types without entering an infinite loop or emitting a false `TS2506` error (unless it is a structurally invalid type cycle).
*   **Emit Fidelity Analysis:** Ensure that when `tsz --build` operates in the global graph backend, the output `.js` and `.d.ts` files perfectly align with the `outDir` structure dictated by the respective original `tsconfig.json` bounds.

### 7.2 Memory Benchmarking
*   **Peak RSS Monitoring (`crates/tsz-cli/src/build.rs`):** We will track Peak RSS across identical builds.
*   *Test Scenario:* Compile a simulated 500-project monorepo (1M+ lines of code) with `check_files_parallel`.
*   *Validation:* The memory ceiling must remain flat (determined by `GlobalTypeInterner` capacity + `N * NodeArena`, where `N` is the thread count, rather than `Total_Files * NodeArena`).

## 8. Gradual Implementation Strategy (The Strangler Fig)

A "big bang" rewrite of the checker and binder would stall feature development and destabilize the project. Instead, we will transition to the Global Query Graph through a sequence of non-breaking, incremental phases. The test suite must remain green at every step.

### Phase A: Symbol Location Decoupling (Non-Breaking)
**Goal:** Break the strict `Symbol` -> `NodeIndex` memory dependency without dropping ASTs yet.
1. Add `file_id` and `span` (start/end bytes) to the `Symbol` struct alongside the existing `declarations: Vec<NodeIndex>`.
2. Update the `Binder` to populate these new fields during `parse_and_bind_parallel`.
3. Introduce a `NodeLocator` utility in the `Checker`. Update the checker's resolution logic to look up nodes via `(FileId, Span) -> NodeIndex` instead of reading `Symbol.declarations` directly.
4. Once all code paths use the `NodeLocator`, safely remove `declarations: Vec<NodeIndex>` from `Symbol`.
*Result: Symbols are now memory-independent. No performance change, but structural readiness is achieved.*

### Phase B: Concurrent Global Indexing (CPU Optimization)
**Goal:** Eliminate the Amdahl's Law bottleneck in `MergedProgram`.
1. Replace `merge_bind_results` with a concurrent merging strategy.
2. Instead of sequential merging, worker threads write directly to a global `DashMap<Atom, SymbolId>` (or `DefId`) during the bind phase.
3. Keep the `Arc<NodeArena>`s alive for now (do not evict yet).
*Result: Immediate reduction in wall-clock time for large monorepos due to concurrent merging. Memory usage remains high.*

### Phase C: The AST Eviction Pool (Memory Optimization)
**Goal:** Implement the Two-Tier Memory Model.
1. Wrap the `Vec<Arc<NodeArena>>` (currently in `MergedProgram`) in an LRU Cache managed by the `QueryDatabase`.
2. Hook the cache up to an RSS monitor (or set a hard limit like `max_arenas = Num_Cores * 2`).
3. Implement the `Hydrate` logic: if the `NodeLocator` requests a file that was evicted, the Virtual File System re-reads the file, re-parses it, and returns the new `NodeArena`.
*Result: Peak RSS drops massively. The compiler can now theoretically compile infinitely large monorepos without OOM crashes.*

### Phase D: Demand-Driven Cycle Resolution (Correctness)
**Goal:** Allow `tsz` to handle circular dependencies without static topological sorting.
1. Shift the `Checker` from a push-model to a pull-model. When evaluating an imported symbol, use `QueryDatabase::evaluate_type(DefId)`.
2. Implement the active-query stack in the `QueryDatabase`.
3. Add the Fixpoint Iteration fallback: when a thread detects it is querying a `DefId` already in its active stack, it traps the cycle, isolates the SCC, and resolves it sequentially.
*Result: Cross-package cycles no longer crash the compiler. Synthetic cycle tests turn green.*

### Phase E: CLI Control Flow Inversion (The Final Cutover)
**Goal:** Delete the old `tsc --build` emulation loop.
1. Add the `--global-graph` flag to the CLI.
2. When active, bypass `tsconfig.json` reference sorting. Pass all files directly to the `QueryDatabase`.
3. Implement the Deterministic Diagnostics Buffer.
4. Once validated against large real-world repos (e.g., Azure SDK), make `--global-graph` the default behavior and deprecate sequential project builds.
*Result: Zero-config monorepo parallelism is achieved.*