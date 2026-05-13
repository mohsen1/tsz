# Claim - DelegateCrossArenaSymbol source-file direct interface slice

**Owner:** Codex session
**Branch:** `codex/perf-delegate-cold-read-cache-20260513`
**Draft PR:** #6231
**Sequences after:** #6212 (annotated variable source-file symbol cache)
**Input decision record:** [`perf-runs/2026-05-13-delegate-variable-symbol-cache.md`](../perf-runs/2026-05-13-delegate-variable-symbol-cache.md)

## Goal

Reduce the 498 stable source-file cold reads left after #6212 by replacing a
safe subset with a direct typed query before `DelegateCrossArenaSymbol` creates
a child checker.

## Initial scope

1. Keep the stable source-file symbol-arena proof as the entry gate.
2. Reuse direct interface lowering only for source-file interfaces whose member
   annotations are scope-independent.
3. Reject source-file interfaces that require target-file symbol resolution,
   such as member annotations that reference local interfaces/imports.
4. Re-run `monorepo-006` attribution and record the child-checker delta.

## Non-goals

- No direct lowering for arbitrary source-file interfaces.
- No direct variable annotation lowering in this slice.
- No declaration-file target work.
- No timing-mode claim against `tsgo`; this remains attribution-mode checker
  work.

## Exit criteria

1. Focused tests cover accepted and rejected source-file interface shapes.
2. `direct_interface_lowering_outcomes.success` rises on `monorepo-006`.
3. `DelegateCrossArenaSymbol` drops on `monorepo-006` without changing the
   diagnostic count.
4. A decision record captures the result and next target.

