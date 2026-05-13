# Claim: Actual-lib indexed utility aliases avoid one declaration-file child checker

Date: 2026-05-13

## Claim

A tightly guarded direct path for the measured actual-lib `FlatArray` utility
alias with an indexed type-literal body and conditional index reduces monorepo-006
`DelegateCrossArenaSymbol` child-checker constructions from 28 to 27 while
keeping diagnostics stable at 10,198.

## Evidence

- Decision record:
  `docs/plan/perf-runs/2026-05-13-delegate-lib-utility-aliases.md`
- Raw diagnostics JSON:
  `docs/plan/perf-runs/raw/2026-05-13-delegate-lib-utility-aliases-monorepo-006-diag.json`
- Raw perf-counter JSON:
  `docs/plan/perf-runs/raw/2026-05-13-delegate-lib-utility-aliases-monorepo-006-pc.json`

## Scope

This is a conservative proof slice, not a general lib-alias solution.
`IteratorResult`, `Record`, and broader shape-only routing were rejected after
hosted conformance regressed on the exploratory branch. The admitted path is
both name-limited to the measured `FlatArray` row and shape-gated on the actual
lib alias body. The larger fix should be a broader canonical actual-lib
type-alias body query or prepopulated canonical `DefinitionStore` entry.
