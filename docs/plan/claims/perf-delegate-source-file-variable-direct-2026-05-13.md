# Claim - DelegateCrossArenaSymbol source-file variable direct slice

**Owner:** Codex session
**Branch:** `codex/perf-delegate-source-file-variable-direct-20260513`
**Draft PR:** #6243
**Sequences after:** #6231 (source-file direct interface query)
**Input decision record:** [`perf-runs/2026-05-13-delegate-source-file-direct-interface.md`](../perf-runs/2026-05-13-delegate-source-file-direct-interface.md)

## Goal

Remove the remaining 251 source-file variable `DelegateCrossArenaSymbol`
child-checker constructions on `monorepo-006`.

## Initial scope

1. Keep the stable source-file symbol-arena proof as the entry gate.
2. Directly lower annotated source-file variables only when their annotation is
   scope-independent or names a same-file interface accepted by the direct
   source-file interface query.
3. Preserve interface annotations as lazy interface types rather than replacing
   them with the eager object body.
4. Re-run `monorepo-006` attribution and record the child-checker delta.

## Non-goals

- No direct lowering for inferred variables.
- No arbitrary target-file name resolution for variable annotations.
- No declaration-file target work.
- No timing-mode claim against `tsgo`; this remains attribution-mode checker
  work.

## Exit criteria

1. Focused tests cover accepted same-file interface annotations and rejected
   type-alias annotations.
2. `delegate_miss_classification.by_kind.variable` drops on `monorepo-006`.
3. `DelegateCrossArenaSymbol` drops on `monorepo-006` without changing the
   diagnostic count.
4. A decision record captures the result and next target.

