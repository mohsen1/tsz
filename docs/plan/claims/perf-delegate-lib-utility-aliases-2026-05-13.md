# Claim: Actual-lib FlatArray avoids one declaration-file child checker

Date: 2026-05-13

## Claim

A tightly guarded direct path for the actual-lib `FlatArray` type alias reduces
monorepo-006 `DelegateCrossArenaSymbol` child-checker constructions from 28 to
27 while keeping diagnostics stable at 10,198.

## Evidence

- Decision record:
  `docs/plan/perf-runs/2026-05-13-delegate-lib-utility-aliases.md`
- Raw diagnostics JSON:
  `docs/plan/perf-runs/raw/2026-05-13-delegate-lib-utility-aliases-monorepo-006-diag.json`
- Raw perf-counter JSON:
  `docs/plan/perf-runs/raw/2026-05-13-delegate-lib-utility-aliases-monorepo-006-pc.json`

## Scope

This is a conservative proof slice, not a general lib-alias solution.
`IteratorResult` and `Record` were rejected after hosted conformance regressed
on the broader exploratory branch. The larger fix should be a dedicated
actual-lib type-alias body query or prepopulated canonical `DefinitionStore`
entry, not a larger name allowlist.
