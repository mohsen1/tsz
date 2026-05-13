# Claim - DelegateCrossArenaSymbol residue classification

**Owner:** Codex session
**Branch:** `codex/perf-delegate-residue-classification-20260513`
**Draft PR:** #6203
**Sequences after:** #6191 (stable source-file symbol-arena cache reuse)
**Input decision record:** [`perf-runs/2026-05-13-delegate-bucket-empty-attribution.md`](../perf-runs/2026-05-13-delegate-bucket-empty-attribution.md)
**Follow-up decision record:** [`perf-runs/2026-05-13-delegate-residue-classification.md`](../perf-runs/2026-05-13-delegate-residue-classification.md)

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

## Implemented slice

This PR adds `source_file_symbol_arena_cache_eligibility` to perf-counter text
and JSON output, wired at the stable source-file symbol-arena cache gate.

On `monorepo-006`, the 828 remaining `DelegateCrossArenaSymbol` child-checker
constructions split into:

- 247 stable source-file cache keys that are cold first reads;
- 540 source-file variable symbols rejected by the current class/interface-only
  stability proof;
- 41 declaration-file targets.

The next implementation target is the 540 variable-symbol slice. A follow-up
must prove a requester-independent variable subset before widening the stable
source-file symbol-arena cache key.

## Non-goals

- No broad cache-key relaxation beyond the #6191 stable subset without a
  requester-independence proof.
- No default timing-mode claims.
- No changes to `TypeEnvironmentCore`.

## Exit criteria

1. Perf-counter JSON and text output expose enough detail to split the 828
   remaining `DelegateCrossArenaSymbol` constructions.
2. A follow-up decision record under `docs/plan/perf-runs/` gives a concrete
   next implementation target.
3. This slice is attribution-only; no target counter reduction is claimed.
