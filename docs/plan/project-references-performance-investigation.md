# RFC: Fast Composite-Mode Compilation & Caching for TSZ

**Status**: Draft (Revised)  
**Owner**: TSZ team  
**Date**: February 20, 2026  
**Audience**: Compiler, build, and LSP maintainers  
**Scope**: Investigation and design (Strict alignment with TSZ North Star v1.2)

## 1. Executive Summary

TSZ's performance within a single project is gated by its efficient `Arena` and `TypeInterner` models. However, at the solution scale (large monorepos), TSZ currently suffers from sequential orchestration, coarse-grained cache invalidation, and redundant type computation across project boundaries.

This RFC proposes a roadmap to achieve fast composite-mode compilation by introducing memory-aware parallel orchestration and semantic cross-project caching. 

**Core Directives for this Initiative:**
1. **Strict North Star Alignment**: Cross-project invalidation and API fingerprinting must respect the `Solver`/`Checker`/`Binder` boundaries. The `Checker` orchestrates; the `Solver` defines the API fingerprint.
2. **Correctness Over Speculation**: We will implement strict, deterministic parallel scheduling and graph invalidation first. Speculative execution is quarantined behind hard gates.
3. **Memory Safety First**: Highly parallel builds using TSZ's `Arena` allocation model risk out-of-memory (OOM) crashes. Scheduling must be memory-aware, not just CPU-aware.

---

## 2. Scope and Constraints

### 2.1 Goals
1. Reduce composite/reference build wall time with zero semantic parity regressions.
2. Introduce a **Memory-Aware Parallel Solution Scheduler**.
3. Implement **Semantic API Fingerprinting** using the `Solver` to minimize downstream recomputation when upstream internals change but public types do not.
4. Unify the invalidation graph between the CLI (`--build`) and the long-lived LSP process, including an explicit memory-eviction strategy for the `TypeInterner`.

### 2.2 Non-Goals
1. Bypassing the `Solver` for fast-path type equivalence checks.
2. Storing raw `TypeId`s or `NodeIndex`es in `.tsbuildinfo` (these are arena-specific and non-deterministic across runs).
3. Treating `.tsbuildinfo` as a stable public format contract.

---

## 3. North Star Architectural Alignment

To implement solution-scale caching without violating TSZ's `NORTH_STAR.md` mandates, we must establish the following invariants:

### 3.1 DefId and API Fingerprinting
The "Public API" of a project cannot be determined purely by AST syntax because of return-type inference. 
* The **Binder** identifies which `SymbolId`s are exported.
* The **Checker** maps these to `DefId`s.
* The **Solver** computes the canonical `TypeId` graph for these `DefId`s.
* **The API Fingerprint** is a deterministic hash of this canonical type graph, oblivious to local `TypeInterner` assignment order.

### 3.2 CLI vs. LSP Memory Models
* **CLI (`--build`)**: Operates on a short-lived memory model. Worker threads may use isolated `TypeInterner`s per project. Cross-project dependencies rely on serialized API fingerprints or parsed `.d.ts` boundaries.
* **LSP**: Operates on a long-lived `global_interner` (North Star 2.2). The invalidation strategy must account for `TypeInterner` bloating. When a file is invalidated, we cannot easily "delete" types from the append-only `TypeInterner`.

---

## 4. Proposed Plan: Strict Architecture, High Performance

### 4.1 Work Package A: Memory-Aware Parallel Scheduler
**Goal**: Maximize CPU utilization without triggering OOM limits via Arena allocations.

**Design**:
1. **DAG Ready Queue**: Build a ready queue from the project graph based on `tsconfig.json` references.
2. **CPU & Memory Token Budgeting**: 
   * Dispatch ready projects to worker threads based on available CPU tokens.
   * **Crucial Addition**: Track system RSS / memory pressure. If the combined `NodeArena`, `SymbolArena`, and `TypeInterner` capacities exceed 80% of system RAM, the scheduler must suspend dispatching new projects until finished projects are dropped from memory.
3. **Deterministic Output**: Buffer diagnostics and standard output per project, emitting them strictly in topological order to match `tsc -b`.

### 4.2 Work Package B: Resilient Multi-Signal Status Oracle
**Goal**: Make no-op `--build` essentially instantaneous, surviving `git checkout` and CI restoration.

**Design**:
* **Tier 1 (Fast-Path Reject)**: Check file `mtime` and size. If they match the cache, the file is unchanged.
* **Tier 2 (The `mtime` Trap Recovery)**: In CI or after a Git checkout, `mtime` will change even if the file content is identical. If `mtime` mismatches, **do not immediately invalidate**. Hash the file content. If the content hash matches the cached hash, update the stored `mtime` and treat the project as clean.
* **Tier 3 (Configuration Check)**: Hash the compiler options, resolution context, and environment variables.
* **Tier 4 (Upstream Check)**: Check if upstream project API Fingerprints (see WP C) have changed.

### 4.3 Work Package C: Semantic API Fingerprinting via Solver
**Goal**: Stop the cascading rebuild of downstream projects when an upstream project changes its private implementation.

**Design**:
1. When a project finishes building, the `Checker` requests the `Solver` to evaluate all exported `DefId`s.
2. The `Solver` traverses the type graph of these exports, creating a canonical, position-independent semantic hash (the "API Fingerprint").
3. **Rule Enforcement**: This calculation *must* happen inside the `Solver`. The `Checker` must not pattern-match on `TypeKey`s to try and guess the public API.
4. Downstream projects rebuild *only* if their upstream dependencies' API Fingerprints change (or if their own source/config changes).

### 4.4 Work Package D: BuildInfo & Type Serialization
**Goal**: Persist necessary state to disk without leaking internal Arena indices.

**Design**:
1. Expand `.tsbuildinfo` schema to capture the resolved root set, resolved references, and the full configuration fingerprint.
2. Store the **API Fingerprint Hash** in the buildinfo.
3. **Boundary Rule**: Never write raw `TypeId`, `SymbolId`, or `NodeIndex` to disk. These are session-specific Arena indices. Map everything to deterministic string paths or content hashes before serialization.

### 4.5 Work Package E: Build/LSP Unification & Epoch Garbage Collection
**Goal**: Share the invalidation graph between CLI and LSP while preventing LSP memory leaks.

**Design**:
1. Use the same API fingerprinting logic for LSP workspace module invalidation.
2. **LSP Memory Mitigation**: Because the LSP uses a `global_interner` for types and long-lived `NodeArena`s, continuous editing will cause unbounded memory growth. We must introduce an **Epoch** concept to the `Project` orchestrator.
   * When a critical mass of files are invalidated, or RSS hits a threshold, the LSP spins up a new `Project` epoch with a fresh `TypeInterner` in the background, populates the active files, and seamlessly swaps pointers, dropping the old interner.
3. Support LSP 3.17 `$/cancelRequest` by integrating cancellation tokens into the `Checker`'s AST traversal loop and the `Solver`'s `MAX_SUBTYPE_DEPTH` recursion limits.

### 4.6 Work Package F: Guarded Speculative Execution
**Goal**: Keep cores busy near critical-path boundaries. *(Implementation deferred to Phase 4)*.

**Design**:
1. Dependents may start building under provisional upstream API fingerprints (assuming the upstream API will not change).
2. If the upstream project finishes and its API Fingerprint *did* change, discard the speculative downstream `NodeArena`/`TypeInterner` state and replay.
3. **Strict Gate**: Off by default. Requires rigorous telemetry proving a replay rate of `<= 20%`.

---

## 5. Performance and Responsiveness Targets

* **No-op Composite Build**: >= 3.0x faster than current TSZ baseline.
* **Leaf Non-API Edit Rebuild**: >= 2.0x faster than current TSZ baseline on Benchmark suites (Azure SDK for JS).
* **API-Change Fan-Out Rebuild**: >= 1.3x faster than baseline while maintaining zero parity regressions.
* **Peak Memory Stability**: <= 1.6x current sequential build RSS. The scheduler's memory-aware backpressure must prevent OOMs on 16-core runners.
* **LSP Cancellation Latency**: `p95 < 120ms` from cancel receipt to work stop (requires cancellation checks inside `Checker` traversal loops).

---

## 6. Concrete Implementation Map

1. **Scheduler/Orchestrator (`crates/tsz-cli/src/build.rs`)**: Implement `DAG` ready-queue. Inject memory-pressure heuristics to pause `rayon` or thread-pool dispatch.
2. **Status Oracle (`crates/tsz-cli/src/incremental.rs`)**: Update `ChangeTracker` to use the `mtime -> fallback hash` resiliency pattern. 
3. **API Fingerprinter (`crates/tsz-solver/src/`)**: Add a new `visitor` in the Solver that recursively visits a `TypeId`, hashing its structural `TypeKey`s to generate a canonical `u64` semantic hash. Expose this via a `Checker` query boundary.
4. **LSP Unification (`crates/tsz-lsp/src/project.rs`)**: Adopt the CLI's dependency invalidation data. Implement the "Epoch Swap" for the `global_interner` to prevent long-term memory leaks.
5. **Cancellation Tokens**: Plumb `CancellationToken` through `CheckerContext` (Section 4.5 of North Star) and into the `Solver`'s state machine.

---

## 7. Critical Risks and Mitigations

| Risk | Impact | Mitigation |
| :--- | :--- | :--- |
| **Arena OOM in Parallel** | Fatal CI crashes on wide graph builds. | Implement Memory-Aware token dispatching (WP A). Do not blindly dispatch to all CPU cores. |
| **Inference Leaks** | Downstream builds incorrectly skipped because an inferred return type changed silently. | API Fingerprint (WP C) relies strictly on the `Solver` fully evaluating all `Lazy(DefId)` boundaries before hashing. |
| **LSP Memory Leak** | IDE crashes after 2 hours of typing. | Implement Epoch-based GC swapping for the `global_interner` (WP E). |
| **mtime CI Storms** | 100% cache miss rate in CI pipelines. | Never invalidate solely on `mtime` mismatch without verifying content hash (WP B). |

## 8. Decision Record
1. **Approved**: Parallel scheduler must be memory-aware, not just CPU-aware.
2. **Approved**: `mtime` is used as a skip-heuristic only; content hashes are the ultimate source of truth.
3. **Approved**: The `Solver` owns the generation of API Fingerprints via a structured type visitor; the `Checker` cannot bypass this.
4. **Deferred**: Speculative execution is strictly Phase 4 and default-off.