# Code-Grounded Plan: Large-Repo Performance

**Status**: Working design, grounded in current code  
**Owner**: TSZ team  
**Date**: March 8, 2026  
**Audience**: Compiler, CLI, and LSP maintainers

## 1. North Star

The goal of this plan is simple: make `tsz` materially faster and flatter in memory on very large repositories without breaking `tsc` compatibility.

That means the plan must be driven by the code we actually have today:

- Multi-file compilation still retains all file arenas after merge in `src/parallel.rs`.
- The checker still runs with borrowed `NodeArena` + `BinderState` and uses cross-arena delegation rather than on-demand hydration.
- `QueryDatabase` is a solver/type-query interface, not a workspace scheduler.
- Project references already have a compatibility frontend in the CLI.

Any plan that ignores those facts will produce architecture churn instead of measurable performance wins.

## 2. What Exists Today

### 2.1 Current parallel pipeline

The current flow is:

`parse_and_bind_parallel -> merge_bind_results -> check_files_parallel`

Key facts from code:

- `BindResult` retains an `Arc<NodeArena>` per file in `src/parallel.rs`.
- `MergedProgram` retains `BoundFile.arena`, `symbol_arenas`, and `declaration_arenas` in `src/parallel.rs`.
- `check_files_parallel` reconstructs a per-file binder from merged global state instead of using a demand-driven workspace engine.

This is a valid baseline, but it means memory currently scales with the number of retained arenas, not just the number of active workers.

### 2.2 Current checker reality

The checker is not yet shaped for AST eviction:

- `CheckerContext` owns `&NodeArena` and `&BinderState`.
- Cross-file work uses `all_arenas`, `all_binders`, and `cross_file_symbol_targets`.
- Cross-file symbol/type resolution creates child checkers via cross-arena delegation.

That is a workable architecture for correctness, but not yet for re-parse-on-demand execution.

### 2.3 Current incremental footholds

We already have two important building blocks for large-repo performance work:

- CLI cache invalidation uses export hashes and dependent tracking in `crates/tsz-cli/src/driver/core.rs`.
- LSP `Project` already does export-signature based smart invalidation in `crates/tsz-lsp/src/project/core.rs`.

Those are not the final global query graph, but they are real performance primitives and should be expanded, not discarded.

## 3. Landed In This Session

This document is not purely aspirational. Two small pieces landed to support future work:

### 3.1 Deterministic project-reference scheduling without repeated re-sorting

`ProjectReferenceGraph::build_order` now uses a heap-backed ready queue instead of re-sorting a `Vec` after every node release.

Why this matters:

- project-reference builds are still part of the current compatibility path,
- stable scheduling is required for trustworthy large-repo benchmarking,
- this removes avoidable `queue.sort()` work from the hot path.

Guardrail test added:

- `test_build_order_keeps_sibling_dependencies_deterministic`

### 3.2 Merged-program residency counters

`MergedProgram::residency_stats()` now exposes stable counters for how much arena-backed state the current pipeline retains after merge.

Why this matters:

- we need baseline numbers before attempting AST eviction or skeletonization,
- this gives us a cheap, testable signal for “how much multi-file state are we still holding?”

Guardrail tests added:

- `test_merged_program_residency_stats_track_unique_file_arenas`
- `test_merged_program_residency_stats_deduplicate_shared_arena_handles`

## 4. Revised Plan

### Phase 0: Measure and protect current behavior

**Goal:** make future large-repo work evidence-driven.

Actions:

1. Keep adding residency and invalidation counters around existing whole-program paths.
2. Add tests that lock in smart invalidation behavior for body-only edits, comment-only edits, private-symbol edits, and export-surface edits.
3. Add benchmark scenarios that report retained arena counts, symbol counts, and invalidated dependent counts alongside wall-clock time.

Why this phase comes first:

- Without stable counters, we cannot tell whether a refactor improved anything.
- Without guardrail tests, “performance work” will regress correctness or invalidate too much cached state.

### Phase 1: Stabilize identity before changing execution

**Goal:** introduce a true file-stable semantic identity layer before any AST eviction work.

Current obstacle:

- `Symbol` still stores `Vec<NodeIndex>` and `value_declaration: NodeIndex`.
- `DefId` creation still happens lazily in the checker from `SymbolId`.

Actions:

1. Extend binder-owned symbol identity with stable declaration locations.
2. Move toward binder/index-owned `DefId` creation for top-level semantic definitions.
3. Keep `NodeIndex` as a local execution detail, not a cross-file semantic identity.

Success criteria:

- checker does not need to invent identity for imported/top-level declarations,
- symbol identity survives re-parse,
- the type environment can resolve `Lazy(DefId)` without checker-local repair logic.

### Phase 2: Build a file skeleton layer, not a full workspace query engine

**Goal:** reduce retained state early without rewriting the checker all at once.

Current obstacle:

- `merge_bind_results` performs more than one job: symbol remapping, declaration merging, export/re-export propagation, augmentation stitching, and declaration-to-arena indexing.

Actions:

1. Define a binder-produced skeleton IR containing only the data needed for:
	- top-level exports,
	- ambient/global/module augmentations,
	- declaration merge candidates,
	- stable definition identity and source span.
2. Split current `merge_bind_results` into:
	- map: per-file skeleton extraction,
	- reduce: deterministic global merge of skeletons,
	- legacy path: full arena-backed merged program for features that still require it.
3. Keep the reduce phase deterministic and ordered. Do not replace it with a naive concurrent `DashMap` merge.

Success criteria:

- we can construct a global index without retaining every user AST,
- declaration merging behavior stays deterministic,
- global augmentations and re-exports remain correct.

### Phase 3: Expand incremental API-fingerprint invalidation across CLI and workspace state

**Goal:** reduce unnecessary downstream work in large repos before touching checker execution.

Current footholds:

- CLI export hashes in `CompilationCache`
- LSP export signatures in `Project`

Actions:

1. Align the CLI and LSP notion of public API fingerprint so they do not drift semantically.
2. Add dependent invalidation summaries to watch/build diagnostics for perf analysis.
3. Broaden tests around re-exports, namespace exports, type-only export changes, and ambient augmentations.

Success criteria:

- body-only edits stay local,
- export-surface changes invalidate the minimal required dependent set,
- cache churn becomes explainable and observable.

### Phase 4: Pull semantic work behind query boundaries before AST eviction

**Goal:** make the checker less arena-centric a subsystem at a time.

This is not “replace the checker with Salsa.” It is a staged conversion of the hottest cross-file paths into demand-driven queries.

Actions:

1. Identify the highest-cost cross-file queries:
	- imported symbol type resolution,
	- class/interface heritage resolution,
	- namespace/module export lookup,
	- lazy generic body evaluation.
2. Move those paths behind boundary helpers that consume stable definition identity and skeleton/global index data first.
3. Keep full-file checking arena-backed until the hot cross-file edges no longer assume all arenas are resident.

Success criteria:

- the checker can answer selected cross-file questions without cloning large binder/arena state,
- cross-arena delegation becomes the fallback path rather than the primary mechanism,
- correctness remains locked by existing tests plus new targeted regression tests.

### Phase 5: Introduce bounded arena residency

**Goal:** reduce peak RSS only after identity and cross-file access are stable.

Actions:

1. Separate pinned libraries from evictable user-code arenas.
2. Introduce a bounded arena pool for user files.
3. Re-hydrate user AST/binder state on demand using stable definition locations.
4. Keep per-worker execution local; do not globalize mutable checker state.

Success criteria:

- peak RSS grows with active work rather than total repository size,
- library types remain hot,
- re-hydration does not cause pathological churn on common edit/check loops.

### Phase 6: Only then consider a real workspace scheduler

**Goal:** replace project-level orchestration once the semantic substrate can support it.

At this point the question is no longer “can we imagine a global query graph?” but “do we have stable IDs, a skeleton index, bounded arena residency, and deterministic diagnostics?”

If yes, then a workspace-level engine can own:

- file discovery,
- dependency scheduling,
- concurrent query execution,
- deterministic diagnostic collection,
- compatibility-mode project-reference validation.

Until then, the current CLI/LSP orchestration should remain the outer control plane.

## 5. Explicit Non-Goals For Now

The following are not near-term steps:

- replacing the current solver `QueryDatabase` with a workspace scheduler,
- deleting project-reference support,
- promising package-cycle support before stable identity and cycle handling exist,
- doing emit via a global graph before check-only mode proves out.

## 6. Validation Strategy

Every phase should prove three things:

1. **Correctness**
	- Conformance unchanged or improved.
	- No new cross-file regressions in checker, binder, or LSP tests.

2. **Performance**
	- Wall-clock time improves on large synthetic and real-world repositories.
	- Dependency invalidation stays proportional to public API change.
	- Arena residency counters move in the intended direction.

3. **Determinism**
	- Build order is stable.
	- Diagnostics are emitted in a stable order.
	- Merged/global indexing produces stable symbol identity and merge results.

## 7. Immediate Next Work

The next meaningful implementation steps are:

1. Add more residency counters and benchmarks around `MergedProgram` and file checking.
2. Unify API-fingerprint semantics between CLI incremental compilation and LSP project updates.
3. Design a binder-owned skeleton IR with stable definition identity and deterministic reduce semantics.
4. Start moving the hottest cross-file checker paths off direct arena/binder residency assumptions.

This is the shortest path that is both compatible with the current codebase and pointed at the real large-repo bottlenecks.