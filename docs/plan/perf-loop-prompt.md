# Perf loop prompt — operating instructions

This is the canonical input to `/loop` for the perf campaign on tsz.
It captures the mission, the architectural direction, design constraints
from review feedback, and operational hygiene. The /loop skill re-enters
on each wakeup with this prompt as `args`.

## Mission

Make tsz materially faster than tsgo on large repos (target: ≥2× on
large-ts-repo, not just single-file cases). The benchmark is
`scripts/bench/bench-vs-tsgo.sh`. Every change must pass
`scripts/session/verify-all.sh` (accept pre-existing failures only).

## Architectural learnings (do not regress)

The dominant lesson from prior iterations: **stop cloning files,
start indexing definitions.** Recent perf work has been mostly symptom
relief — Arc-wrapping per-file maps, parallelizing startup passes,
pre-sizing hash maps — and yet tsz still OOMs on the 6086-file
large-ts-repo. The blocker is architectural, not micro-optimization.

The "original sins" identified in review:

1. **File as the unit of execution.** Per-file arenas are retained
   after merge; per-file binders are reconstructed from merged state;
   `CheckerState` carries `&NodeArena`/`&BinderState`. File objects
   and arena residency are the runtime abstraction; they should be
   stable semantic definitions and queries.
2. **Execution identity leaked into semantic identity.** `Symbol.value_declaration: NodeIndex`
   ties identity to arena residency. `DefId` creation still happens
   lazily in the checker. `NodeIndex` should be a local execution
   detail, not a cross-file semantic identity.
3. **Global state copied into local binders.** Program-wide tables
   (module_exports, declaration_arenas, lib_symbol_ids,
   alias_partners, declared_modules, ...) get cloned/re-materialized
   into per-file binders — the "copy the world into worker-local
   context" anti-pattern.
4. **Memoization at the wrong scope.** `instantiate_type` constructs
   a fresh `TypeInstantiator` per call; its `visiting` cache is
   call-local. Sibling callers redo the same work. Hot cross-file
   operations need caches keyed by stable semantic identity, not
   call site.
5. **Parallelism before architectural compression.** Parallelizing
   passes multiplies a heavyweight file-centric architecture across
   cores instead of shrinking the architecture itself.

The **6-phase plan** (`docs/plan/global-query-graph-architecture.md`):
- Phase 0: Measure and protect current behavior (residency counters, invariant tests)
- Phase 1: Stabilize identity before changing execution (binder-owned `DefId`, stable declaration locations)
- Phase 2: File skeleton IR — deterministic reduce without retaining all arenas
- Phase 3: API-fingerprint invalidation unified across CLI and LSP
- Phase 4: Pull semantic work behind query boundaries
- Phase 5: Bounded arena residency (libraries pinned, user arenas evictable)
- Phase 6: Real workspace scheduler

**Status**: Phase 1 step 1 (`StableLocation` on `Symbol`) shipped as PR #1045.
Several Phase 4 prerequisites in flight (instantiate_type cache PRs).
Continued Arc-share work is acceptable as Phase 0 plumbing but should not
displace the architectural pivot.

## Design constraints from review (instantiate_type cache)

Design doc: `docs/plan/perf-instantiate-type-cache-design.md` (PR #1007 merged).
Five invariants must be preserved in any implementation:

1. **Cache hooks on `QueryDatabase`, not `TypeDatabase`.** The codebase
   designates `QueryDatabase` as the cache/incremental boundary
   (`crates/tsz-solver/src/caches/db.rs:636`). PR 3 must thread
   `&dyn QueryDatabase` through the five entry points; do NOT widen
   `TypeDatabase` with cache hooks even though it's mechanically
   convenient.
2. **`substitute_this_type` carve-out.** It always passes
   `TypeSubstitution::new()` (empty) but carries a non-empty `this_type`.
   The "skip cache when subst.is_empty()" rule must be:
   `skip only when substitution.is_empty() && this_type.is_none()`.
3. **Do NOT intern substitutions on `TypeInterner`.** `QueryCache`
   doesn't own the interner; `clear()` and `estimated_size_bytes()`
   can't see interner state. Use `CanonicalSubst(SmallVec)` directly
   in the key for v1. If profiling demands dedup later, intern on
   `QueryCache` itself.
4. **Cross-interner `TypeId` comparison is meaningless.** `TypeId` is a
   `u32` interner-local handle. Tests must stay within one interner;
   for cross-interner cross-checks use `DisplayType::to_string` or
   structural walk.
5. **Preserve leaf fast paths.** `instantiate_type` has bespoke fast
   paths for `TypeParameter` direct hits and `IndexAccess(T, P)` at
   `instantiate.rs:1449–1468`. Cache-key construction MUST run after
   these, not before. `instantiate_generic` is out of scope unless
   `application_eval_cache` overlap is explicitly addressed.

## Workflow

1. **Pull latest main first.** Main moves fast (10+ PRs/hour from
   parallel agents). `git fetch origin main && git rebase origin/main`
   on every iteration.
2. **Spawn parallel Opus teammates** in worktree isolation for
   independent perf work. Each runs `verify-all.sh` + targeted bench
   before pushing. Salvage stalled agents (token-limit / watchdog) by
   inspecting their worktree commits, rebasing, and pushing yourself.
3. **Verify before pushing.** `cargo check --workspace` +
   `cargo clippy --workspace --all-targets --all-features -- -D warnings`
   + `cargo nextest run -p <touched crates> --lib` at minimum.
   `scripts/session/verify-all.sh` is the gate. Conformance regressions
   are non-negotiable.
4. **Bench every shippable change.** Use `bench-vs-tsgo.sh` (or `hyperfine`
   on a specific case) to confirm the change moves a number, not just
   feels right. Discard speculative perf changes that fall within noise
   — that's symptom relief.
5. **Don't bail.** If a change is hard, dig deeper. Multi-PR migrations
   are expected. If main has moved past a PR, rebase and resolve cleanly.
6. **Open PRs.** Never push to main. Title format
   `perf(<area>): ...` for perf, `arch(<area>): ...` for architectural
   pivots, `docs(<area>): ...` for design docs.

## Disk hygiene (NEW)

When disk usage is ≥75%, before starting new compile-heavy work:

```bash
df -h /Users/mohsen | head -3
# If Capacity ≥ 75%, clean stale agent worktree caches:
for w in /Users/mohsen/code/tsz/.claude/worktrees/agent-*/; do
    branch=$(git -C "$w" branch --show-current 2>/dev/null)
    # Only clean stale agents (worktree-agent-*); preserve PR worktrees (perf/*, arch/*)
    if [[ "$branch" == worktree-agent-* ]]; then
        rm -rf "$w/.target" "$w/target"
    fi
done
df -h /Users/mohsen | head -3  # confirm freed
```

Each agent's `.target` is ~1–6 GB. Stale agents accumulate quickly
when running parallel teammates. Never delete a worktree's git state
or branch with uncommitted work — only clean cargo caches.

## What's been shipped this campaign (track here)

### Architectural / docs:
- #1007 — design: instantiate_type cross-call cache (revised after review)
- #1011 — refresh: perf followup with 2026-04-23 state
- #1051 — canonical loop prompt with architectural learnings + review constraints
- #1053 — loop prompt update: #1043 merge + Phase 2 finding + ops learnings
- #1062 — loop prompt update: forbid idle waiting + concrete alt-work list

### Phase 1 — STABILIZE IDENTITY (2026-04-24/25): **COMPLETE** ✅
- **#1055** — Phase 1 step 1: `StableLocation` on `Symbol` (parallel to `NodeIndex`, populated in lockstep, 12 bytes, survives arena drop)
- **#1066** — Phase 1 step 2: migrate `class_extends_any_base` from `NodeIndex` to `StableLocation`; introduce `CheckerContext::node_at_stable_location((file_idx, pos, end)) -> Option<(NodeIndex, &NodeArena)>` rehydration helper. **Phase 5 unblocks here** — user arenas can now be evicted because consumers can rehydrate `NodeIndex` on demand from a stable triple.

### Arc-share migrations (Phase 0 plumbing — eliminates per-file deep clones):
- **MERGED**: #932 lib_symbol_ids • #944 wildcard_reexports • #954 module_exports
- **MERGED**: #960 lib_binders • #973 module_augmentations • #979 global_augmentations
- **MERGED**: #986 symbol_arenas • **#1039 flow_nodes** (2026-04-24)
- **MERGED**: **#1043 declaration_arenas + sym_to_decl_indices** (2026-04-24)
- **MERGED**: **#1064 resolved_modules** (2026-04-24) — eliminates ~120K String clones × N files

### Cache infra (Phase 4 prerequisites):
- **#1040 MERGED** — PR 1/4: canonical-pairs `TypeSubstitution` (deterministic content-hashable form)
- **#1128 MERGED** (2026-04-25) — PR 2/4: `InstantiationCache` storage on `QueryCache` + `lookup_instantiation_cache`/`insert_instantiation_cache` on `QueryDatabase`. No entry-point wiring.
- **#1132 OPEN** (2026-04-25) — PR 3/4: wire 5 entry points via `_cached` variants + `Option<&dyn QueryDatabase>` parameter (deviation from literal spec — strict signature change blocked by 116 cascading errors; `_cached` variants land same perf win without multi-day rewrite). Bench results vs prior tsz: paths.ts 115→58ms (-50%), deep-pick.ts 200→53ms (-74%, **flips tsgo-faster→tsz-faster 1.89×**), deep-readonly.ts 99→62ms (-37%). 6 hot-path callers wired in TypeEvaluator + SubtypeChecker.

### Phase 1 — STABLE IDENTITY (continued, 2026-04-25):
- **#1131 MERGED** (2026-04-25) — Phase 1 step 3: `identifier_source_display` migration (2 functions, 3 new tests including Phase 5 round-trip across arena reparse).

### Phase 2 — SKELETON IR consumers:
- **#1127 MERGED** (2026-04-25) — Phase 2 step 1: `is_ambient_module` resolver served from `SkeletonIndex` alone (Phase 5 invariant test included). First skeleton consumer migrated.
- **#1135 MERGED** (2026-04-25) — Phase 2 step 2: `global_module_augmentations_index` migration. Enriches `SkeletonAugmentation` with `Vec<(name, StableLocation)>`, adds `module_augmentations_for(spec)` accessor + `build_module_augmentations_index(arenas)` projection helper on `SkeletonIndex`. CLI driver pre-computes the index and installs on `ProjectEnv`; `build_global_indices` skips per-binder loop when pre-built map is present. 19,014 pre-commit tests pass; same Phase 5 unblock pattern as #1127.
- **#1138 MERGED** (2026-04-25) — Phase 2 step 3: `global_augmentation_targets_index` migration. Enriches `FileSkeleton` with `SkeletonAugmentationTarget {symbol_id, module_spec, stable_location}`. Adds `augmentation_targets_for(spec)` accessor and `build_augmentation_targets_index()` projection helper. Same `ProjectEnv` skip-when-pre-built pattern. 13,224 pre-commit tests pass. Required rebase on top of #1135 (26 conflicts — union strategy: both indexes coexist).
- **#1141 MERGED** (2026-04-25) — Phase 2 step 4: `global_module_binder_index` migration. `module_spec → [file_idx]` rebuildable from skeleton.
- **#1162 MERGED** (2026-04-25) — Phase 2 step 6: `global_module_exports_index` migration. `binder.module_exports` is intentionally empty on cross-file binders so this projection is safe (unlike #1145's `file_locals` which carries SymbolIds). `ProjectedModuleExportsByName` + `ProjectedModuleExportsIndex` type aliases. 13,246 pre-commit tests pass.

### Bench-vs-tsgo (2026-04-25, post Cache PR 3 + Phase 2 steps 1-4):
**Score: tsz 13 wins vs tsgo 3 wins.** 13 of 16 measurable tests show tsz faster.
- **utility-types/index.ts: 1.83× faster** (closest to 2× target)
- ts-essentials/paths.ts: 1.59×; ts-toolbelt/Iteration: 1.26×; many ~1.10–1.24×
- **REGRESSIONS**: DeepPartial optional-chain N=50: 1.25× SLOWER; Shallow optional-chain N=50: 1.21× SLOWER.
- **large-ts-repo: TIMED OUT at 300s** — the actual 2× target case still doesn't complete. Phase 5 arena eviction still required.

### large-ts-repo TIMEOUT root cause (2026-04-25, HIGH confidence):
**Failure mode: OOM-by-paging.** macOS `sample` shows tsz allocating **67 GB virtual on 32 GB RAM** at t=27s; jetsam kills 17 background daemons (TrustdAgent, CommCenter, dmd) due to system-wide memory pressure; tsz exits silently at ~71-75s (no diagnostics, empty stdout/stderr).

**Hot stack (>2400/3900 samples)**: `collect_diagnostics` → `Vec<Arc<BinderState>>::from_iter` (serial!) → `create_cross_file_lookup_binder_with_augmentations` (`crates/tsz-cli/src/driver/check_utils.rs:1673`) → `BinderState::from_bound_state_with_scopes_and_augmentations` (`crates/tsz-binder/src/state/core.rs:500`) → **`BinderState::recompute_module_export_equals_non_module` (`core.rs:1533`)** → `hashbrown::RawTable<(SymbolId, SemanticDefEntry)>::clone` + `Vec<String>::clone` + mimalloc churn.

**Memory is O(files × modules × symbols)**: each per-file binder deep-clones the cross-file module-exports map. At 6086 files × hundreds of modules × thousands of symbols → 50-70 GB.

**Fix path** (HIGH leverage, ROOT CAUSE CORRECTED 2026-04-25):
1. ~~Arc-share `module_export_equals_non_module`~~ — INCORRECT. Verified: this map is EMPTY on cross-file lookup binders (their `module_exports` is empty by design at `check_utils.rs:1722`). The `RawTable<(SymbolId, SemanticDefEntry)>::clone` stack frames were misattributed — they come from `file.semantic_defs.clone()`, NOT this recompute.
2. **Arc-share `semantic_defs`** — the REAL hotspot is `binder.semantic_defs = file.semantic_defs.clone()` at `check_utils.rs:1651` and `:1766`. Each file's `SemanticDefEntry` (String + Vec<String> fields) is unique, but the per-file map can be wrapped in `Arc<FxHashMap<SymbolId, SemanticDefEntry>>` and stored on `BoundFile` so cross-file binder construction does `Arc::clone` instead of element-wise deep-clone. Or intern strings into `Atom` to make clones cheap.
3. **Other unshared per-binder fields**: `node_symbols`, `node_flow`, `node_scope_ids`, `top_level_flow`, `switch_clause_to_switch` are all FxHashMaps cloned per file. Same Arc-share pattern applies. The 67 GB peak is parallel-rayon-worker × per-binder-size (many large unshared maps).
4. **Tear down `Vec<Arc<BinderState>>` entirely** — finish the Phase 2 migration so per-file `BinderState` reconstruction is no longer needed (the architectural endgame).
5. **Stop-gap**: chunk-process files (256 at a time), drop each chunk's binder vec before next — caps peak RSS at chunk_size × per-binder.

**Threading anomaly** (worth a separate look): only 1 hot worker thread at ~98% CPU; 11 rayon workers idle. The `par_iter` at `check.rs:317-334` is structurally parallel but the sample shows a serial fold path. Likely a serial second cross-file binder build elsewhere in `collect_diagnostics`, OR rayon starved by mimalloc allocator contention.

### Optional-chain regression — root cause investigation (2026-04-25):
Hypothesis (Cache PR 3 cache-overhead) FALSIFIED — Shallow has no generics yet regresses identically with DeepPartial. Real cause: **request cache has 0% hit rate** (`Request cache hits: 0`, `misses: 8535`, `Contextual cache bypasses: 6206`). But the **direct fix attempt also failed**: extending `request_cache_key_for_node` + `is_request_cache_safe_expression_tree` in `state.rs:1069-1318` for `BINARY_EXPRESSION (??/+/-/*//)` and `PARENTHESIZED_EXPRESSION` was implemented + tested + conformance-clean, but bench showed 1.02-1.03× SLOWER (pure overhead, zero new cache hits). Each `score += …` line is a DISTINCT AST node id — never revisited → no hits possible.

The actual bypass count is dominated by `is_audited_contextual_request_cache_kind` in `expr.rs:95-99`, NOT `request_cache_key_for_node` in state.rs. Productive next step: **profile node-id histogram** on the fixture to see WHICH nodes are bypassed and IF ANY are revisited, then either extend the expr.rs audit list (different predicate) or target a different bottleneck (e.g., property-access evaluation per chain rung).
- **#1145 CLOSED** (2026-04-25) — Phase 2 step 5: `global_file_locals_index` migration introduced a CONFORMANCE REGRESSION (6 shards failing on retrigger — not flaky).
  - **ROOT CAUSE (HIGH confidence, 2026-04-25 investigation)**: the cross-file lookup binders the checker iterates are RECONSTRUCTED post-merge in `crates/tsz-cli/src/driver/check_utils.rs:1685-1702`. They contain `program.file_locals[file_idx]` (REMAPPED to global SymbolIds) PLUS `program.globals` (lib symbols like Array, console, replicated per file_idx). The skeleton extracts from PRE-MERGE binders (`crates/tsz-core/src/parallel/skeleton.rs:430-449`) which have only file's own locals with LOCAL SymbolIds (don't index into the global SymbolArena post-merge). So PR #1145's projection had WRONG SymbolIds AND MISSING global replicas.
  - **WHY #1135/#1138/#1141 WORKED**: those indexes carry `module_spec → file_idx[]` only — no SymbolIds, no global-namespace replication.
  - **Fix paths** (NOT YET IMPLEMENTED): (1) project skeleton from POST-merge topology (lose some Phase 5 unblocking), or (2) migrate consumers off `(file_idx, SymbolId)` to name-only re-resolve via `all_binders[file_idx].file_locals.get(name)` (bigger refactor).
  - **`module_exports_index` next-target risk: LOW** — `binder.module_exports` is intentionally empty on cross-file lookup binders; the legacy loop is already a no-op for that field; data flows through `program.module_exports` instead.
  - Branch preserved at `phase2-step5-file-locals-index`. **DO NOT REOPEN without applying one of the fix paths above.**

### Solver hot-path optimizations:
- **#1125 [open]** — `remove_subtypes_for_bct` name-fingerprint pre-filter: skip impossible subtype pairs in O(N²) loop without invoking `SubtypeChecker`. Conservative — only definitive negatives short-circuit. (Salvaged from stalled BCT agent.)

### Profiling infra:
- **#1065 MERGED** — `flame` Cargo profile (`debug=2, strip=false, codegen-units=1, lto="thin"`) for `samply` / `cargo flamegraph`.

### Bench infra:
- #988 partial JSON on OOM/TERM
- #1004 parallelize build_cross_file_binders

## Phase 2 status — ALREADY ~70% PLUMBED (2026-04-24 finding)

`crates/tsz-core/src/parallel/skeleton.rs` is **1135 lines** of skeleton
infrastructure:
- `FileSkeleton` (per-file extracted from `BindResult`)
- `SkeletonIndex` (post-reduce; merge candidates, augmentation targets,
  re-export graph, ambient/shorthand modules)
- `extract_skeleton`, `reduce_skeletons` — deterministic pipeline
- `diff_skeletons` — incremental diff
- `validate_against_merged` — runs in debug builds, asserts skeleton
  captures the same topology as the legacy `MergedProgram` merge

Currently the skeleton is built **alongside** the legacy merge and only
validated against it (debug-only). The remaining Phase 2 work is **migrating
consumers off the legacy `MergedProgram` path** so user arenas can be
evicted (Phase 5 prerequisite). With Phase 1 complete (`StableLocation` +
`node_at_stable_location` rehydration helper landed in #1055/#1066), Phase 2
consumers can use the same rehydration pattern. The next architectural
target is to make ONE skeleton consumer arena-free — same proof-of-concept
shape as Phase 1 step 2 PR #1066.

## Operational learnings (2026-04-24)

- **DO NOT SIT IDLE waiting for CI**. Heartbeats should be ≤10 min when
  there's a pending PR close to merging, AND the iteration must always
  find concrete non-conflicting work — never just status-check and sleep.
  Concrete options when blocked on CI:
  1. Start a local `bench-vs-tsgo.sh` run in the background to measure
     actual current perf after recent merges.
  2. Spawn a teammate in worktree isolation on a non-conflicting perf
     target (e.g. BCT algorithm review in `inference/infer_bct.rs`).
  3. Add `tracing` spans to the Check phase for attribution — no
     profiler needed, just instrumented timings.
  4. Find pre-existing clippy/lint errors on main and ship `chore(...)`
     fixes.
  5. Read solver hot paths looking for obvious O(N²) patterns.
- **CI sometimes wedges**: `tsz-pr-unit` checks can stick "pending" for hours
  with stale build IDs. **Verified working remediation**: `git rebase
  origin/main && git push --force-with-lease` triggers fresh build IDs.
  After force-push the unit check completed in 7m3s as normal. Threshold:
  build ID unchanged + >60 min since last push.
- **Profiling on macOS without sudo is hard**: `samply --save-only` produces
  unsymbolicated profiles even with `RUSTFLAGS="-C debuginfo=2"`; `nm` on
  stripped Rust release binary mostly returns `OUTLINED_FUNCTION_*` names;
  `cargo flamegraph` needs dtrace/sudo. Need a Cargo.toml profile with
  `debug = 2, strip = false` AND interactive `samply load` to symbolicate,
  OR add `tracing` spans into the Check phase and read those.
- **Disk hygiene works**: cleaning stale agent `.target` dirs freed 17 GB
  in one pass (79% → 77% capacity).
- **Clippy chore PRs can become redundant**: if another PR ships the same
  fix, your rebase drops the commit with "patch contents already upstream".
  Verify via grep on main — if the fix is there, close as redundant.

## Anti-patterns (don't do)

- **Pre-sizing hash maps** — already covered in main; further pre-size
  changes typically fall within noise. Verified -1.2% (within ±2%) on
  manyConstExports; abandoned.
- **Speculative changes without measurement** — every shipped perf PR
  must show a concrete before/after delta from `hyperfine` or
  `bench-vs-tsgo.sh`. "It should help" is not enough.
- **Widening `TypeDatabase`** — see design constraint #1.
- **Caching on `TypeInterner`** — see design constraint #3.
- **Cross-interner `TypeId` equality in tests** — see design constraint #4.
- **Treating Arc-share as the goal** — it's prerequisite plumbing for
  Phase 5 residency work, not the win itself.

## Open questions / next concrete work

1. **Cache PR 3/4 (IN FLIGHT)** — wire the 5 `instantiate_type*` entry
   points to the cache landed in #1128. Constraints in design doc §5.
   Agent: `cache-pr3-wire-opus`. This is where the perf win lands on
   utility types (`ts-essentials/deep-readonly.ts` cited as 16.56×
   gap pre-fix).
2. **Cache PR 4/4** — optional shared cross-file cache on
   `SharedQueryCache`. Gate on PR 3 stats showing cross-file
   hit rate is non-trivial.
3. **Phase 2 step 2** — pick the next skeleton consumer. Candidates:
   `lookup_by_name` (currently iterates all 6086 binders per call —
   `crates/tsz-checker/src/state/type_resolution/module.rs:2070-2087`),
   global augmentations resolver, or wildcard re-export graph.
4. **Phase 1 step 3 (IN FLIGHT)** — migrate `identifier_source_display`
   from `NodeIndex` to `StableLocation`. Agent has +354/-36 staged but
   no commits yet (status nudge sent).
5. **Phase 3** — CLI/LSP fingerprint unification. `compute_export_signature`
   in `crates/tsz-cli/src/project/incremental.rs` may be able to route
   through `tsz_lsp::ExportSignature`. tsbuildinfo serialization format
   constrains this — needs care.
6. **Known followup (NOT IN ANY PR)** — `lookup_by_name` global name
   index. Iterates all 6086 binders per call. Fix: global
   `Arc<FxHashMap<&str, SmallVec<[(BinderIdx, SymbolId); 2]>>>` built
   at merge, plumb through `CheckerContext`. Multi-hour task; defer
   until Cache PR 3/4 lands (potential conflict on checker fields).
7. **~~3.28× regression on manyConstExports — RESOLVED 2026-04-25:
   was a CLI invocation error, not a regression.~~** Original measurement
   used `tsz check --noemit <file>` which is invalid (CLI treats `check`
   as a filename, hits TS6053 file-not-found error path). Correct form is
   `tsz --noemit <file>`. Real measurement on current main (post-#1128):
   80.8 ms ± 3.4 ms vs claimed 77.3 ms baseline = within noise. No
   regression exists.

This document should evolve. When a directive lands wrong (regression,
review change, design pivot), update this file and re-feed it as the
loop prompt.
