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

### 7.3 Incremental Performance Benchmarks
*   **Semantic API Cache Hit Rate:** Introduce an end-to-end benchmark in `benches/parallel_bench.rs` to mutate a function body (an internal change) within an upstream project. Verify that the cache invalidation graph is successfully pruned by the Semantic API Fingerprint, and that NO downstream projects re-run the `check_files_parallel` phase.