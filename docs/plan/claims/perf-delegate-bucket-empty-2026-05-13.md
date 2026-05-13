# Claim - DelegateCrossArenaSymbol bucket-empty follow-up

**Owner:** Codex session
**Branch:** `codex/perf-delegate-bucket-empty-20260513`
**Draft PR:** #6191
**Sequences after:** #6144 (TypeEnvironmentCore arena-direct type-param extraction)
**Input decision record:** [`perf-runs/2026-05-13-typeenv-arena-direct-attribution.md`](../perf-runs/2026-05-13-typeenv-arena-direct-attribution.md)
**Follow-up decision record:** [`perf-runs/2026-05-13-delegate-bucket-empty-attribution.md`](../perf-runs/2026-05-13-delegate-bucket-empty-attribution.md)

## Goal

Reduce the remaining scale-cliff child-checker construction path:
`with_parent_cache_by_reason[DelegateCrossArenaSymbol]`.

After #6144, `TypeEnvironmentCore` drops to one construction per fixture, and
the largest remaining reason is `DelegateCrossArenaSymbol`: 924 constructions
on `monorepo-006`. The #6111 source-file symbol-arena cache path made these
misses observable as `cross_file_cache_miss_causes.bucket_empty`, but did not
produce reusable `delegate.cache_hits_cross_file` hits.

## Initial scope

1. Reproduce the post-#6144 monorepo-006 attribution signal on current
   `origin/main`.
2. Classify the 924 `DelegateCrossArenaSymbol` misses by:
   - source-file vs declaration-file target;
   - symbol kind;
   - `symbol_arenas` vs `declaration_arenas` vs symbol-file target source;
   - whether the source-file symbol-arena cache key is missing because of
     requester-file scoping, type-parameter payload, unsupported direct
     lowering, or no writer.
3. Implement the smallest safe fix supported by that classification.

## Implemented slice

The first fix keeps the existing program scope in the source-file symbol-arena
cache key but removes requester-file scoping for the already-proven stable
subset: single-declaration class/interface symbols, no module augmentations, and
empty type-parameter payloads.

On `monorepo-006`, this converts 96 `bucket_empty` misses into cross-file cache
hits and drops `DelegateCrossArenaSymbol` constructions from 924 to 828. The
smaller fixtures have no repeat requester pattern for this stable subset and
remain unchanged.

## Non-goals

- No changes to the #6144 TypeEnvironmentCore path.
- No broad cache-key relaxation without a proof that the answer is
  requester-independent for the targeted subset.
- No timing-mode comparison to `tsgo`; this slice uses attribution-mode
  counters only until the child-checker reason drops.

## Exit criteria

1. Follow-up decision record under `docs/plan/perf-runs/` explains the measured
   `DelegateCrossArenaSymbol` delta.
2. Targeted implementation reduces `DelegateCrossArenaSymbol` constructions on
   `monorepo-006` from 924 to 828.
3. Targeted tests cover the stable program-scoped cache key.
4. CI is green before merge.
