# The Parallelism and Determinism Model

tsz parallelizes the embarrassingly-parallel parts of compilation — parsing,
binding, lib loading, skeleton extraction, and per-file checking — across a
Rayon worker pool, but it has one absolute constraint: **the diagnostics,
inference results, and emit it produces must be byte-identical to a single
sequential run, and identical to `tsc`.** Parallel scheduling is allowed to
change *when* work happens; it is never allowed to change *what* the answer is.
This chapter is the gap-filler for that contract. It documents the
`tsz-core` parallel subsystem (`dep_graph`, `residency`, `lib_snapshot`,
`skeleton`, `core`), the Rayon fan-out the CLI driver runs across
parse/bind/check, the shared-arena isolation contract that lets many checker
threads write into one `TypeInterner`/`DefinitionStore` safely, the exact
conditions under which the driver *refuses* to parallelize and falls back to
sequential checking for determinism, and how per-file diagnostics are gathered
and re-sorted into a stable program order before rendering.

This extends [end-to-end-timeline](end-to-end-timeline.md), which names the
sequence of driver calls without going deep on the scheduler. It is also the
twin of [driver-incremental-and-watch](driver-incremental-and-watch.md) (which
owns cache invalidation) and
[driver-project-references-and-build-mode](driver-project-references-and-build-mode.md)
(which owns `--build` ordering). The kernel stages that the workers run are
owned by [front-end-scanner-parser](front-end-scanner-parser.md), [binder](binder.md),
[checker-context-and-state](checker-context-and-state.md), and the `solver-*`
chapters; the per-thread caches the workers share are owned by
[solver-caches-objects-contextual-compat](solver-caches-objects-contextual-compat.md)
and [solver-types-intern-def](solver-types-intern-def.md). The final
diagnostic comparator and filtering live in
[checker-error-reporter-diagnostics](checker-error-reporter-diagnostics.md) and
[cli-surface-and-diagnostic-reporting](cli-surface-and-diagnostic-reporting.md).

## Owns / Must not own

| The parallelism/determinism layer owns | It must not own |
| --- | --- |
| Rayon global-pool init and per-workload scoped pools (`ensure_rayon_global_pool`, `run_with_rayon_pool_for_work_items`). | Type semantics. Workers call the same kernel a sequential run would; the schedule never changes a relation, inference, or evaluation result. |
| The `maybe_parallel_iter!` / `maybe_parallel_into!` target switch (Rayon on native, serial on `wasm32`). | Diagnostic *meaning*. Codes/messages/reasons come from checker+solver; this layer only sorts, dedups, and stitches them into program order. |
| The CLI scheduler dispatch in `collect_diagnostics`: fresh-per-file, sequential-reuse, parallel-chunk-reuse, and the cost-balanced checker pool. | The merge algorithm's semantics. `merge_bind_results` is sequential and ordered by construction; this layer only decides what runs in parallel *before* and *after* the merge. |
| The determinism gates: tiny-batch floor, the DOM/webworker order-sensitive-lib refusal, and the env kill switches. | AST/symbol shapes. The schedule never reorders global `SymbolId` assignment — that is fixed by file-discovery order upstream (see [end-to-end-timeline](end-to-end-timeline.md)). |
| Result re-assembly into original `work_items` order and the canonical `Diagnostic::compare` re-sort. | Per-file kernel caches. `QueryCache` (`RefCell`/`Cell`) is single-threaded by construction; `SharedQueryCache`/`TypeInterner`/`DefinitionStore` are `DashMap`-backed and owned by the solver. |

## Where the code lives

| Path | Role |
| --- | --- |
| `crates/tsz-core/src/parallel/mod.rs` | Module root; re-exports `core`, `dep_graph`, `diagnostics`, `lib_snapshot`, `residency`, `skeleton`. |
| `crates/tsz-core/src/parallel/core/parse_and_libs.rs` | Rayon pool init (`ensure_rayon_global_pool`), scoped small-workload pool (`run_with_rayon_pool_for_work_items`), the `maybe_parallel_*` macros, parallel parse (`parse_files_parallel`) and parallel bind (`parse_and_bind_parallel*`), and parallel lib load. |
| `crates/tsz-core/src/parallel/core/checking.rs` | Reusable parallel-check harness (`check_files_parallel`, `ParallelCheckPlan`) and the per-file/per-lib diagnostic sort+dedup. Not the production CLI scheduler. |
| `crates/tsz-core/src/parallel/core/merge_support.rs` | `merge_bind_results*` — the **sequential, ordered** reduction of per-file bind results into `MergedProgram`. |
| `crates/tsz-core/src/parallel/core/bind_result_reducer.rs` | `BindResultReducer` and `merge_bind_results_from_source` — the single-pass deterministic merge body. |
| `crates/tsz-core/src/parallel/skeleton/mod.rs` | `FileSkeleton` extraction (parallel above a threshold) and `reduce_skeletons` (deterministic, ordered) producing the `SkeletonIndex`. |
| `crates/tsz-core/src/parallel/dep_graph.rs` | `DepGraph` + `topological_order` (Kahn's algorithm, cycles via iterative Tarjan), used for sequential dependency-first ordering. |
| `crates/tsz-core/src/parallel/lib_snapshot.rs` | Disk-backed parse+bind cache for lib files (`TSZ_LIB_CACHE`), so the parallel lib-load phase is skipped on a hit. |
| `crates/tsz-core/src/parallel/residency.rs` | `MergedProgramResidencyStats` — retained-arena/declaration accounting for eviction budgeting and `--extendedDiagnostics`. |
| `crates/tsz-cli/src/driver/check.rs` | The production scheduler: `collect_diagnostics`, the dispatch between fresh/sequential-reuse/chunk-reuse/checker-pool arms, and all determinism gates. |
| `crates/tsz-cli/src/driver/check_file.rs` | The per-file check bodies the arms call: `check_file_for_parallel`, `check_files_sequentially_with_reuse`, `check_files_in_parallel_chunks_with_reuse`, `check_files_cost_balanced_pool`, `lpt_bin_assignment`. |
| `crates/tsz-cli/src/driver/sources.rs` | Parallel source read/scan (`read_source_files` level-synchronous BFS) and parallel lib clone. |
| `crates/tsz-cli/src/driver/source_resolution_setup.rs` | Parallel module-resolution post-pass over the resolved-specifier map. |
| `crates/tsz-common/src/diagnostics/mod.rs` | `Diagnostic::compare` / `compare_skip_related_information` — the canonical total order every final sort runs through. |
| `crates/tsz-common/src/limits/mod.rs` | `THREAD_STACK_SIZE_BYTES` (128 MiB) — the worker stack size that survives deep type-level recursion. |

## The Rayon pool: one global, scoped exceptions

There is one process-global Rayon pool, initialized lazily and exactly once
through a `std::sync::Once` (`RAYON_POOL_INIT`) in
`ensure_rayon_global_pool` (`crates/tsz-core/src/parallel/core/parse_and_libs.rs`).
Laziness matters: a single-file sequential run never pays pool-startup cost.
The headline detail is the stack size — every worker thread is built with
`tsz_common::limits::THREAD_STACK_SIZE_BYTES` (128 MiB, far above the OS
default 8 MiB). Type-level libraries (ts-toolbelt, ts-essentials) produce
deeply nested conditional/mapped evaluation chains where every
`evaluate -> evaluate_application -> instantiate -> evaluate` cycle consumes
real stack frames even with logical recursion guards in place; the oversized
stack is what keeps a worker from overflowing where the main thread would not.

Two carve-outs from the global pool exist:

- **Small-workload scoped pool** (`run_with_rayon_pool_for_work_items`): for a
  bounded number of independent items (`work_item_count <=
  SMALL_WORKLOAD_RAYON_MAX_ITEMS`, currently 32), it builds a transient pool
  capped at `SMALL_WORKLOAD_RAYON_THREADS` (4) so tiny generated-app projects
  do not pay full-width worker-startup overhead, while the process-global pool
  stays at default width for later larger projects. An explicit
  `RAYON_NUM_THREADS` in the environment forces this off (the user's width
  wins), as does an item count of 0 or one over the small cap.
  (`rayon_worker_count_for_work_items`).
- **Lib-load pool** (`parse_and_bind_lib_files`): the lib set parses+binds on a
  pool capped at `min(available_parallelism, 8, file_count)` workers, because
  there are only ~87 lib files and beyond ~8 workers scheduler overhead
  dominates.

On `wasm32` there are no threads at all. The `maybe_parallel_iter!` and
`maybe_parallel_into!` macros expand to `.par_iter()`/`.into_par_iter()` on
native and `.iter()`/`.into_iter()` on `wasm32`, and `ensure_rayon_global_pool`
is a no-op. This is deliberate: WASM consumers (conformance harness, website)
drive their own worker-level parallelism (Node worker threads), so an inner
Rayon pool would oversubscribe and crash/OOM.

```text
ensure_rayon_global_pool()         once, lazily, via std::sync::Once
   └─ ThreadPoolBuilder::new()
        .stack_size(128 MiB)        ← survives ts-toolbelt recursion depth
        .build_global()

run_with_rayon_pool_for_work_items(n, f)
   ├─ n ∈ (0, 32]  &&  !RAYON_NUM_THREADS  →  transient pool, ≤4 workers
   └─ otherwise                            →  global pool
```

## Where the fan-out happens, stage by stage

Parallelism appears at five points along the pipeline. Each point preserves
output order either by Rayon's order-preserving `collect` (output index ==
input index) or by an explicit stable re-sort downstream.

```text
  ┌─────────────────────────────────────────────────────────────────────┐
  │ 1. SOURCE READ + SCAN  (read_source_files, sources.rs)              │
  │    level-synchronous BFS: each level's files read+scanned in        │
  │    parallel (par_iter), then a SERIAL resolver phase mutates        │
  │    seen/pending in original BFS order  → discovery order stable     │
  ├─────────────────────────────────────────────────────────────────────┤
  │ 2. PARSE + BIND  (parse_and_bind_parallel*, parse_and_libs.rs)      │
  │    maybe_parallel_into!(files).map(bind_file…).collect()            │
  │    one NodeArena + BindResult per file, output order == input       │
  │    LIBS: parse_and_bind_lib_files on ≤8-worker pool (or snapshot)   │
  ├─────────────────────────────────────────────────────────────────────┤
  │ 3. SKELETON EXTRACT + MERGE  (skeleton/mod.rs, merge_support.rs)    │
  │    extract_skeletons_for_merge: par_iter ABOVE 128 files            │
  │    reduce_skeletons + merge_bind_results: SEQUENTIAL, ORDERED       │
  │       → MergedProgram (the single semantic universe)               │
  ├─────────────────────────────────────────────────────────────────────┤
  │ 4. CROSS-FILE BINDER BUILD  (check.rs)                              │
  │    ≤32 files: serial;  >32 files: par_iter().enumerate()           │
  │    output indexed by file_idx → order stable                       │
  ├─────────────────────────────────────────────────────────────────────┤
  │ 5. CHECK  (collect_diagnostics dispatch, check.rs + check_file.rs)  │
  │    fresh-per-file / sequential-reuse / chunk-reuse / checker-pool   │
  │    each arm preserves work_items order; diagnostics re-sorted       │
  │    through Diagnostic::compare before rendering                     │
  └─────────────────────────────────────────────────────────────────────┘
```

The merge at stage 3 is the synchronization barrier: everything before it is
per-file-independent, and `merge_bind_results` folds the per-file bind results
into one `MergedProgram` in a single sequential pass
(`merge_bind_results_from_source`, `bind_result_reducer.rs`). The reducer's
doc note is explicit that the merge moves order-dependent per-context work to
merge time as a "single pass, deterministic" operation. `reduce_skeletons`
carries the same contract: it is a pure function whose "same input skeletons (in
the same order) always produce the same output," and every `FileSkeleton` field
it consumes is itself sorted deterministically at extraction time (by name, by
`(name, pos, end)`, by `(module_spec, symbol_id)`) so that `HashMap` iteration
order can never leak into the merged index.

## The shared-arena isolation contract

After the merge, many checker threads run concurrently against shared,
interior-mutable state. The contract that makes this safe — and
schedule-independent — is the heart of the determinism model.

**What is shared and thread-safe (sharded `DashMap`).** The `TypeInterner`
(`crates/tsz-solver/src/intern/core/interner.rs`) stores every interned type and
its derived caches (`identity_comparable_cache`, `widen_type_cache`,
`predicate_cache`, `union_normalize_cache`, object property indices, display
caches, variance masks, …) in `DashMap`s with `FxBuildHasher`. The
`DefinitionStore` (`crates/tsz-solver/src/def/core.rs`) likewise keys
`definitions`, `alias_forwards`, `type_to_def`, `symbol_def_index`, etc. on
`DashMap`. Both are designed for "concurrent access from multiple checking
threads." Type *identity* is content-addressed: the same structural type
interns to the same `TypeId` regardless of which worker requests it first, so
two threads racing to intern an identical type converge on one handle.

**What is per-thread (zero-overhead, `RefCell`/`Cell`).** Each file's checker
gets its own `QueryCache` (`tsz_solver::construction::QueryCache::new`),
memoizing `evaluate_type`/`is_subtype_of` calls with single-threaded interior
mutability — no atomics on the hot path. This is constructed per file in
`check_one_file` and `check_file_for_parallel`.

**What is shared but carefully scoped (`SharedQueryCache`).** For multi-file
projects the driver creates one
`tsz_solver::construction::SharedQueryCache`
(`crates/tsz-solver/src/caches/shared_query_cache.rs`), a `DashMap`-backed L2
that sibling per-file checkers consult on local miss. Crucially, **only the
schedule-insensitive caches are shared**: `eval_cache`, `subtype_cache`, and
`assignability_cache`. The inner relation writes are gated by
`cache_definitive!` in the `SubtypeChecker` so only lazy-resolution-stable
results reach the shared store. `application_eval_cache` and
`instantiation_cache` are **deliberately not shared cross-file**: parallel
checking can observe incomplete lib-merge state during the first evaluation of a
generic alias (`Promise<T>`, `Awaited<T>`), and a stale entry would then be
returned to sibling files. Keeping those per-file removes the ordering-sensitive
correctness risk (issue #9507; the experimental `TSZ_SHARE_INSTANTIATION_CACHES=1`
path is #13240's witness). The escape hatch `TSZ_EXPERIMENT_NO_SHARED_QC`
disables the shared cache entirely to isolate cache-poisoning races during
parallel-lane debugging.

```text
   per file (RefCell/Cell)        cross-file (DashMap, thread-safe)
   ┌──────────────┐               ┌────────────────────────────────┐
   │ QueryCache   │── miss ──────▶│ SharedQueryCache               │
   │  eval/sub/   │◀── fill ──────│  eval_cache (definitive only)  │
   │  assign/     │               │  subtype_cache  / assign_cache │
   │  application │   (NOT shared) │  [application/instantiation    │
   │  /instant.   │               │   intentionally per-file]      │
   └──────────────┘               └────────────────────────────────┘
                                  ┌────────────────────────────────┐
   every worker also reads/writes │ TypeInterner   (DashMap)       │
   the single shared universe ───▶│ DefinitionStore(DashMap)       │
                                  │ content-addressed → identity    │
                                  │ converges regardless of order   │
                                  └────────────────────────────────┘
```

The lib clone path enforces a complementary isolation rule. `load_checker_libs`
(`check.rs`) builds *fresh* checker-facing `LibContext`s rather than reusing the
program-binding lib binders, because binding mutates per-file binder state while
injecting lib symbols; reusing those binders would leak binding-phase state into
lib type resolution and corrupt recursive lib relations
(`RegExpMatchArray`, `Promise<T>`, `PromiseLike<T>`).
`clone_lib_files_for_checker` (`parse_and_libs.rs`) shares the lib *binder*
read-only (its resolution caches are `RwLock`-interior and cleared once on the
shared instance) but gives each clone a distinct outer arena `Arc` identity, so
arena-pointer-identity discriminators across the checker behave exactly as they
did under the old deep clone.

## The CLI scheduler: four arms and how the driver picks one

The production check scheduler is `collect_diagnostics` /
`collect_diagnostics_with_source_resolutions` in
`crates/tsz-cli/src/driver/check.rs`. The reusable harness in
`tsz-core/.../checking.rs` (`check_files_parallel`) is explicitly documented as
**not** the production path; feature and fidelity fixes start in the CLI
scheduler. After building `work_items` (file indices to check, with
`node_modules` declaration roots deferred — see below), the driver selects one
of four check arms:

| Arm | Function | When |
| --- | --- | --- |
| **Fresh per-file (parallel)** | `check_file_with_fresh_checker` via `work_items.par_iter().with_min_len(1)` | Default for `>32`-file non-DOM projects when no reuse/pool arm is chosen. One `CheckerState` + `QueryCache` per file. |
| **Fresh per-file (sequential)** | `work_items.iter().map(check_file_with_fresh_checker)` | Tiny batches (`<=32` files) or an order-sensitive global lib is present (DOM/webworker). |
| **Sequential session reuse** | `check_files_sequentially_with_reuse` | Tiny no-emit non-JS projects (opt-in default), or `TSZ_FILE_SESSION_REUSE=1`. One `CheckerState` re-targeted across files via `switch_to_file`. |
| **Parallel chunk reuse** | `check_files_in_parallel_chunks_with_reuse` | `>32` files, no-emit, with `TSZ_FILE_SESSION_REUSE=1`. `par_chunks(8)` of contiguous files, each chunk a reused checker. |
| **Cost-balanced checker pool** | `check_files_cost_balanced_pool` | Default-ON for `>32`-file non-DOM no-emit projects without an explicit reuse opt-in. `pool_size` long-lived checkers, files bin-packed by cost. |

The `with_min_len(1)` on the fresh-parallel path is a deliberate work-stealing
tuning: per-file check time varies wildly (a one-alias file is ~ms; a file that
triggers a deep `delegate_cross_arena_symbol_resolution` cascade through
ts-essentials/react.d.ts is seconds), so the driver forces Rayon to *not*
pre-chunk the file list into large blocks — fine-grained stealing lets idle
workers grab one file at a time from a busy worker's queue rather than gating
the whole batch on the worker that drew the heavy block.

### The cost-balanced checker pool and LPT bin-packing

The default large-project arm
(`check_files_cost_balanced_pool`, `check_file.rs`) is the lever that unblocks
big multi-file projects. Constructing one `CheckerState` per file means paying
the O(program) `ProgramContext::apply_to` setup N times; the pool instead runs
exactly `pool_size` long-lived checkers, each reused across `files / pool_size`
files via `switch_to_file`, so the expensive setup is amortized. The risk it
defends against is **straggler skew**: a static round-robin (`pos % pool_size`)
ignores per-file cost, so one partition can collect a disproportionate share of
the heavy files and bound wall time even when aggregate CPU is fine.

The fix is `lpt_bin_assignment`: estimate each file's cost by AST node count
(`arena.nodes.len()`, already materialized by binding — no extra traversal),
sort heaviest-first, and greedily place each file into the currently-lightest
bin. This is the classic longest-processing-time (LPT) makespan heuristic (a
4/3-approximation), so the busiest balanced bin is provably close to the
`total / pool_size` lower bound. Ties (equal cost, equal bin load) break to the
lowest bin index, which keeps the assignment **deterministic** and independent
of input ordering up to cost. After each partition checks its files, results
carry their original `work_items` position and are stitched back into original
order — so the partitioning, however cost-driven, never changes which
diagnostics are produced or where they sort. The per-partition body is
`check_files_sequentially_with_reuse` verbatim, so the reuse path's diagnostics
are byte-identical to the fresh-checker arm.

```text
files (by AST node count):   [9, 6, 5, 4, 3, 2, 1, 1]   pool_size = 3
LPT greedy (heaviest first into lightest bin):
   bin0: 9        1     = 10
   bin1: 6     3        = 9
   bin2: 5  4     2  1  = 12     ← straggler, but ≈ total/3 = 10.3
   (round-robin pos%3 could have put 9+4+1 = 14 in one bin)
each bin → one long-lived CheckerState (apply_to once, switch_to_file per file)
results stitched back to original work_items order → byte-identical output
```

## When the driver forces sequential checking for determinism

Three gates can pull work off the parallel path. They are policy decisions made
*before* any worker runs, and they exist purely to preserve byte-identical
output, never as a feature flag.

**1. Tiny-batch floor.** `should_use_sequential_fresh_checking` returns true
when `work_item_count <= FILE_SESSION_REUSE_SMALL_PROJECT_MAX_FILES` (32). Below
that, Rayon's pool-startup overhead and the risk of nondeterministic false
positives from concurrent first-time type interning outweigh any parallel
speedup, so small projects stay sequential.

**2. DOM/webworker order-sensitive global lib refusal.** This is the load-bearing
determinism gate. `has_parallel_order_sensitive_global_lib`
(`checker_lib_diagnostics.rs`) tests whether any loaded lib is
`lib.dom.d.ts`/`dom.d.ts`/`lib.webworker.d.ts`/`webworker.d.ts`
(`is_parallel_order_sensitive_global_lib`). If so, both
`should_use_sequential_fresh_checking` forces the sequential fresh path **and**
`pool_refused_for_order_sensitive_global_lib` refuses the bounded checker pool.

The reason is documented at length in the `should_use_sequential_fresh_checking`
comment: the blocker is **in-flight shared-`DefinitionStore` state**. Every
fresh checker re-derives def bodies into the shared store
(last-writer-wins; benign sequentially because each checker reads its own writes
through its `TypeEnvironment` before falling back to the store, and foreign
bodies are only read after their writer completed). Under parallelism, sibling
workers can consume bodies/params *mid-rewrite*, so deferred-type evaluation
(`keyof`/indexed-access/conditional checks) observes half-constructed foreign
forms. The observed failure mode on ts-toolbelt at 4 workers was 0/5 correct
runs — 3/5 livelocked >150s, 2/5 emitted false `TS2344`s — because generic
conditionals resolved to definitive false branches while a type parameter was
still generic, feeding self-sustaining recursive expansions whose accumulator
grew fresh `TypeId`s every step, defeating every `TypeId`-keyed cycle guard.
Two structural fixes landed from that investigation, but several in-flight
channels (delegation buckets, lib cache, interner side-state) still leak, so
DOM/webworker programs keep the deterministic sequential gate until the
mutation-isolation campaign makes shared def state fully schedule-independent.

**3. The emit-cache restriction.** Both reuse arms and the checker pool are
gated on `!extract_type_cache`. `extract_type_cache` is true when emit or
declarations are requested (`!options.no_emit || options.emit_declarations`),
because the `TypeCache` is extracted by *consuming* the `CheckerState`
(`extract_cache(self)`), which a single reused checker held across a whole loop
cannot do. So emit/declaration runs fall to the fresh-per-file arms (which can
consume a fresh checker per file), and the reuse/pool optimizations apply only
to `--noEmit` runs.

### Env knobs that override the gates

These are diagnosis/measurement knobs; the production defaults above never
depend on them.

| Variable | Effect |
| --- | --- |
| `TSZ_DISABLE_FILE_SESSION_REUSE=1` | Force the session-reuse arms off (takes precedence over enable). |
| `TSZ_FILE_SESSION_REUSE=1` | Opt larger projects into reuse arms (legacy pre-#6870 knob). |
| `TSZ_CHECKER_POOL=<n\|auto\|0>` | Explicit pool width / `auto` = `available_parallelism` / `0`=off; explicit width wins over the kill switch (`resolve_checker_pool_size`). |
| `TSZ_DISABLE_CHECKER_POOL=1` | Kill switch for the default-on pool (an explicit width still wins). |
| `TSZ_EXPERIMENT_NO_SHARED_QC` | Disable the cross-file `SharedQueryCache`. |
| `TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK` | Bypass *only* the DOM/webworker gate so forced-parallel byte-diffs can be driven from the env. |
| `TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK_TINY` | Additionally bypass the tiny-batch floor so the schedule-determinism regression guards exercise the real `par_iter` path on small witnesses. |
| `RAYON_NUM_THREADS` | Standard Rayon width override; also disables the small-workload scoped pool so the user's width wins. |
| `TSZ_LIB_CACHE=0` | Disable the disk-backed lib snapshot cache (force parse+bind). |

## Determinism mechanics: how output is held stable

Three independent mechanisms guarantee byte-identical output regardless of
schedule.

**a. Order-preserving fan-out.** Rayon's `par_iter().map(...).collect()`
preserves input ordering: result index `i` corresponds to input index `i`,
independent of which thread finished first. Every parallel stage relies on this
— `parse_files_parallel`, `parse_and_bind_parallel*`, the cross-file binder
build (`par_iter().enumerate()`), and the fresh-parallel check arm. The
`run_file_checks` doc in `checking.rs` states it directly: "`par_iter().enumerate()`
preserves input ordering (`file_idx`) so results are deterministic regardless of
which thread completes first." Where an arm does *not* preserve order
intrinsically (the cost-balanced pool repartitions files), it carries each
file's original position and reassembles into a fully-filled `Vec` indexed by
that position.

**b. Stable upstream ordering of inputs.** `defer_node_modules_declaration_roots`
sorts `work_items` so `node_modules` `.d.ts` files check last, using
`sort_by_key` — a *stable* sort, so the relative order of non-`node_modules`
files (which is file-discovery order) is preserved exactly. The level-synchronous
BFS in `read_source_files` runs the file read+scan in parallel but feeds the
serial resolver phase items "in the original BFS order, so the visited-set
ordering and dependency propagation are unchanged"; `push_unique_dep` preserves
source-import order in dependency lists for cached-rebuild replay. Because global
`SymbolId` assignment follows file-discovery order, holding that order stable is
what makes the whole semantic universe schedule-independent.

**c. Canonical diagnostic re-sort.** Per-file diagnostics are sorted and
deduplicated *within* each file at production time
(`diagnostics.sort_by(|a, b| a.compare(b))` then `dedup_by` on `(start, code)`
in `check_one_file` / `sort_and_dedup`), and the final program-wide list is sorted
through `Diagnostic::compare` (`crates/tsz-common/src/diagnostics/mod.rs`). That
comparator is a **total order over observable fields**, mirroring tsc's
`compareDiagnostics`: by file, then start, then length, then code, then message
text, then related information. Its doc is explicit that this total order is
"what keeps reported diagnostic order deterministic across equivalent relations,
regardless of the (potentially parallel or hash-map-driven) order in which the
diagnostics were produced," and that every emitting site must sort through this
comparator rather than an ad-hoc partial key — otherwise diagnostics that tie on
a partial key fall back to nondeterministic production order. After the per-file
results are gathered, the driver extends the program diagnostic list in
`work_items` order (re-deriving the true `file_idx` via `work_items[idx]`), then
the rendering layer re-sorts the full list through `compare`.

```text
worker A ─┐   per-file: sort_by(compare) + dedup(start,code)
worker B ─┼─▶ Vec<FileCheckResult> (order == work_items)
worker C ─┘        │
                   ▼  extend in work_items order, then
            diagnostics.sort_by(|l,r| l.compare(r))   ← total order
                   ▼
            stable program order, == sequential, == tsc
```

## Lib-snapshot cache and the parallel lib-load shortcut

Lib loading is itself parallel (`parse_and_bind_lib_files`), but the disk-backed
`lib_snapshot` cache often skips it entirely. Before parsing the lib set,
`parse_and_bind_lib_files`' caller computes per-file content hashes
(`content_hash` over `(file_name, source_text)` via `FxHasher`) and calls
`lib_snapshot::try_load_many`; on a hit it deserializes the persisted
`(NodeArena, BinderState)` and returns immediately — skipping **both** parse and
bind. The snapshot format is `[8-byte magic "TSZSNAP\x08"][bincode payload]`;
the trailing magic byte is a version tag that invalidates older snapshots when
the `BinderState`/`NodeArena` layout changes. Resolution caches inside
`BinderState` are `#[serde(skip)]` and repopulate lazily on first lookup, so the
snapshot stores only the durable parse+bind state. The cache is on by default;
`TSZ_LIB_CACHE=0` forces the parse+bind path. Because lib files are sorted
largest-first before fan-out (`file_contents.sort_by_key(Reverse(len))`),
`dom.d.ts` (≈40K lines, 2 MB) starts early under work-stealing instead of being
the late-arriving critical-path bottleneck.

## DepGraph and the sequential dependency cascade

`DepGraph` (`dep_graph.rs`) is built from each `FileSkeleton`'s `import_sources`
and produces a `topological_order` via Kahn's algorithm: files with no in-graph
dependencies seed the queue, dependents decrement as their dependencies are
emitted, and any remaining nodes are cycle members. Cycles are detected and
grouped into strongly-connected components by an **iterative** Tarjan's
algorithm (an explicit call stack rather than recursion, so a deep cycle cannot
overflow the worker stack), then appended to the order "in stable (input) order"
after all acyclic files. This ordering feeds the *sequential* cached-rebuild
path in `check.rs` (the `else` arm of the dispatch, used when no shared
`DefinitionStore` exists — e.g. some tests): `topological_file_order` reorders
the work queue dependency-first so a file's dependencies are checked before it,
maximizing cache/export-hash availability for incremental invalidation
(see [driver-incremental-and-watch](driver-incremental-and-watch.md)). The main
parallel path does not require topological order — the merge already produced one
semantic universe — but the dep graph's edge/root/cycle counts feed residency
reporting.

## Caches and invariants

| Cache / shared state | Owner | Concurrency primitive | Invalidation / scoping invariant |
| --- | --- | --- | --- |
| `TypeInterner` | solver (`intern/core/interner.rs`) | sharded `DashMap` | Content-addressed: identical structure → identical `TypeId`, order-independent. Lazily-allocated maps; no cross-run reset in a single compile. |
| `DefinitionStore` | solver (`def/core.rs`) | `DashMap` | Last-writer-wins on def bodies; safe sequentially because writers complete before foreign reads. The unfinished-isolation gap is exactly what the DOM/webworker gate guards against. |
| `SharedQueryCache` | solver (`caches/shared_query_cache.rs`) | `DashMap` | Only `eval`/`subtype`/`assignability` shared, and only `cache_definitive!` (lazy-resolution-stable) results; `application_eval`/`instantiation` deliberately per-file (issue #9507). Lives for one multi-file check; residency recorded before drop. |
| `QueryCache` | solver `construction` | `RefCell`/`Cell` | Per-file/per-partition, single-threaded; reuse-arm variant shares one across a loop. Cumulative stats emitted once on the last iteration to keep the aggregator's sum-of-per-file == cumulative invariant. |
| `lib_snapshot` (disk) | `parallel/lib_snapshot.rs` | file + content hash | Keyed by `(file_name, source_text)` `FxHasher`; magic-byte version tag invalidates on layout change; `serde(skip)` resolution caches rebuild lazily. |
| `RAYON_POOL_INIT` | `parallel/core/parse_and_libs.rs` | `std::sync::Once` | Global pool built exactly once, 128 MiB worker stacks. |

## Edge cases and tsc parity

- **Single-file fast paths.** `run_file_checks` skips Rayon when
  `program.files.len() <= 1`, and `parse_and_bind_parallel_with_libs_and_target`
  takes a serial path for `<= 1` file (it also only builds the premerged lib
  binder for multi-file projects). The same result as the parallel path, without
  pool overhead.
- **`.json` inputs.** Both the parse-only and bind paths route `.json` files to
  `synthesize_json_parse_result` / `synthesize_json_bind_result` instead of the
  TS grammar, so a `package.json` does not emit spurious `TS1005`/`TS1128`. This
  must hold identically on the parallel and serial branches.
- **Lib re-check is conditional.** After the per-file check, lib files are
  re-checked only when user code augments/extends an affected lib interface
  (`needs_lib_recheck` / `affected_lib_interfaces`). Each affected lib is checked
  twice — a baseline and an augmented run — and only augmentation-induced
  diagnostics survive the `lib_diagnostic_fingerprint` subtraction
  (`(file_name, start, code, message_text)`). This keeps pre-existing lib noise
  out of user output, matching tsc.
- **DOM programs are slower but correct.** The order-sensitive-lib gate means a
  DOM/webworker project loses the parallel/pool speedup. The repo accepts this
  as a deliberate parity-over-speed tradeoff until the mutation-isolation
  campaign lands; the gate is a *correctness* guard, not a feature toggle.
- **WASM is always serial.** The `maybe_parallel_*` macros collapse to serial
  iteration and `ensure_rayon_global_pool` is a no-op, so conformance runs under
  WASM produce the same diagnostics with no inner threads.
- **Determinism is regression-tested.** The schedule-determinism guards in
  `parallel_sequential_agreement_tests` exist precisely to catch a parallel arm
  diverging from the sequential baseline; `TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK_TINY`
  is the knob that forces tiny witnesses onto the real `par_iter` path so the
  guards compare parallel-vs-sequential rather than sequential-vs-sequential.

## Cross-references

- [end-to-end-timeline](end-to-end-timeline.md) — the driver call sequence this
  chapter zooms into.
- [driver-incremental-and-watch](driver-incremental-and-watch.md) — the caches
  and topological order the sequential cascade uses for invalidation.
- [driver-project-references-and-build-mode](driver-project-references-and-build-mode.md)
  — `--build` ordering across projects.
- [module-resolution-engine](module-resolution-engine.md) — the resolver the
  serial BFS phase drives.
- [solver-caches-objects-contextual-compat](solver-caches-objects-contextual-compat.md),
  [solver-types-intern-def](solver-types-intern-def.md) — the `TypeInterner`,
  `DefinitionStore`, and `SharedQueryCache` internals the workers share.
- [checker-error-reporter-diagnostics](checker-error-reporter-diagnostics.md),
  [cli-surface-and-diagnostic-reporting](cli-surface-and-diagnostic-reporting.md)
  — the `Diagnostic::compare` ordering and final rendering.
- [checker-context-and-state](checker-context-and-state.md) — `CheckerState`,
  `CheckerContext::switch_to_file`, and `ProgramContext::apply_to` that the reuse
  arms re-target.
