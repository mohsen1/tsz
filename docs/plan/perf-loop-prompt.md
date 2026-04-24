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

Architectural / Phase 0–1:
- #1007 — design: instantiate_type cross-call cache (revised after review)
- #1011 — refresh: perf followup with 2026-04-23 state
- #1045 — Phase 1 step 1: `StableLocation` on `Symbol`

Arc-share migrations (Phase 0 plumbing — eliminates per-file deep clones):
- #932 lib_symbol_ids • #944 wildcard_reexports • #954 module_exports
- #960 lib_binders • #973 module_augmentations • #979 global_augmentations
- #986 symbol_arenas • #1039 flow_nodes • #1043 declaration_arenas + sym_to_decl_indices

Cache infra (Phase 4 prerequisites):
- #1040 PR 1/4 — canonical-pairs `TypeSubstitution`

Bench infra:
- #988 partial JSON on OOM/TERM
- #1004 parallelize build_cross_file_binders

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

1. **Phase 1 step 2**: migrate ONE consumer from `NodeIndex` to
   `StableLocation` to prove the pattern. Start with
   `crates/tsz-checker/src/types/queries/lib_resolution.rs` (small
   surface, clear ownership). After #1045 merges.
2. **Phase 2**: design a binder-produced skeleton IR (the doc points at
   this; no design PR yet). Should answer: what's the minimum data
   needed for the deterministic reduce that gives stable identity?
3. **Phase 3**: investigate whether `crates/tsz-cli/src/project/incremental.rs`
   `compute_export_signature` can route through `tsz_lsp::ExportSignature`.
   Note: tsbuildinfo serialization format constrains this — needs care.
4. **Profile the Check phase** on `manyConstExports.ts` — 80% of time
   with 0 cache hits means there's an O(N_symbols) per-declaration pass
   we don't have attribution for. Use `cargo flamegraph` or `samply`.
   The current bench gap (1.4× slower than tsgo) on symbol-heavy files
   is unaccounted-for and not a known Arc-share target.

This document should evolve. When a directive lands wrong (regression,
review change, design pivot), update this file and re-feed it as the
loop prompt.
