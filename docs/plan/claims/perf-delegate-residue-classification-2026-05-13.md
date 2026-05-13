# Claim - DelegateCrossArenaSymbol residue classification

**Owner:** Codex session
**Branch:** `codex/perf-delegate-residue-classification-20260513`
**Draft PR:** to be opened with this claim
**Sequences after:** #6191 (stable source-file symbol-arena cache reuse)
**Input decision record:** [`perf-runs/2026-05-13-delegate-bucket-empty-attribution.md`](../perf-runs/2026-05-13-delegate-bucket-empty-attribution.md)

## Goal

Explain and reduce the remaining `DelegateCrossArenaSymbol` child-checker
residue after #6191.

The current measured cliff state is:

- `monorepo-006` `DelegateCrossArenaSymbol = 828`;
- `delegate.cache_hits_cross_file = 96`;
- `cross_file_cache_miss_causes.bucket_empty = 247`;
- all remaining `DelegateCrossArenaSymbol` misses are still from
  `symbol_arenas`.

## Initial scope

1. Reproduce or consume the latest post-#6191 attribution data on current
   `origin/main`.
2. Add narrow attribution for why a `symbol_arenas` delegation cannot avoid a
   child checker:
   - no stable cache key available;
   - stable cache key available but cold;
   - direct lowering rejected by arena kind;
   - direct lowering rejected by symbol shape.
3. Use the new data to pick one small implementation target, or record that the
   next implementation target needs a different counter.

## Non-goals

- No broad cache-key relaxation beyond the #6191 stable subset without a
  requester-independence proof.
- No default timing-mode claims.
- No changes to `TypeEnvironmentCore`.

## Exit criteria

1. Perf-counter JSON or text output exposes enough detail to split the 828
   remaining `DelegateCrossArenaSymbol` constructions.
2. A follow-up decision record under `docs/plan/perf-runs/` gives a concrete
   next implementation target.
3. If an implementation change is made in this slice, it must reduce the
   target counter and have focused tests.
