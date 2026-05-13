# Claim - DelegateCrossArenaSymbol variable-symbol cache slice

**Owner:** Codex session
**Branch:** `codex/perf-delegate-variable-symbol-cache-20260513`
**Draft PR:** to be opened with this claim
**Sequences after:** #6203 (DelegateCrossArenaSymbol residue classification)
**Input decision record:** [`perf-runs/2026-05-13-delegate-residue-classification.md`](../perf-runs/2026-05-13-delegate-residue-classification.md)

## Goal

Reduce the largest measured `DelegateCrossArenaSymbol` residue after #6203:
the 540 source-file variable-symbol child-checker constructions reported as
`source_file_symbol_arena_cache_eligibility.unstable_symbol` on `monorepo-006`.

## Initial scope

1. Audit source-file variable symbols that currently fail
   `symbol_arena_symbol_type_cache_is_stable`.
2. Identify the smallest requester-independent variable subset, if one exists,
   that can safely share through the stable source-file symbol-arena cache key.
3. Implement only that proven subset, with focused tests showing diagnostics and
   type answers stay stable across requesters.
4. Re-run `monorepo-006` attribution and record whether the variable-symbol
   counter drops.

## Non-goals

- No broad cache-key relaxation for arbitrary variables.
- No declaration-file target work; #6203 measured that slice at 41
  constructions, below the variable-symbol target.
- No timing-mode claim against `tsgo`; this is still attribution-mode checker
  work.

## Exit criteria

1. A focused test covers the safe variable-symbol subset or documents why no
   safe subset was found.
2. If implemented, `source_file_symbol_arena_cache_eligibility.unstable_symbol`
   and `DelegateCrossArenaSymbol` drop on `monorepo-006`.
3. A follow-up decision record under `docs/plan/perf-runs/` captures the result
   and next target.
