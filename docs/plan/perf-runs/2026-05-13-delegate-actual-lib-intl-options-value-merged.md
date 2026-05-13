# 2026-05-13 - DelegateCrossArenaSymbol Actual-Lib Intl Option Interfaces

Attribution-mode follow-up on top of
`bdca836172` (`perf(checker): admit value-merged iterator lib interfaces`).

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `bdca836172` |
| after branch | `codex/perf-goal-next-20260513-replay` (this slice) |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| fixture path | `/private/tmp/tsz-bench-fixtures/monorepo-006` |
| counter mode | `TSZ_PERF_COUNTERS=1` |
| machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-13-delegate-actual-lib-iterator-value-merged-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-delegate-actual-lib-iterator-value-merged-after-monorepo-006-pc.json`
- After:
  `docs/plan/perf-runs/raw/2026-05-13-delegate-actual-lib-intl-options-value-merged-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-delegate-actual-lib-intl-options-value-merged-after-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

## Change

The direct actual-lib path now admits a narrow Intl options/registry family
through the existing namespace-qualified resolver path (no broad alias/value
relaxation):

- `DateTimeFormatOptions`
- `NumberFormatOptions`
- `NumberFormatOptionsCurrencyDisplayRegistry`
- `NumberFormatOptionsStyleRegistry`
- `NumberFormatOptionsUseGroupingRegistry`

`Locale` remains on fallback in this slice because it did not lower directly
under the same proof gates.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` children | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 24 | 24 | 974 | 1 | 434 | 24 |
| monorepo-006 after | 18 | 18 | 976 | 1 | 434 | 18 |
| delta | -6 | -6 | +2 | 0 | 0 | -6 |

Diagnostics count is unchanged (`10,198` on both runs).

## Miss Classification

| Bucket | Baseline | After |
| --- | ---: | ---: |
| `target_declaration_files` | 24 | 18 |
| `target_source_files` | 0 | 0 |
| `by_kind.type_alias` | 14 | 15 |
| `by_kind.interface` | 10 | 3 |

Declaration-file residue rows removed:

- `DateTimeFormatOptions`
- `Function`
- `NumberFormatOptions`
- `NumberFormatOptionsCurrencyDisplayRegistry`
- `NumberFormatOptionsStyleRegistry`
- `NumberFormatOptionsUseGroupingRegistry`
- `Object`
- `RegExp`

New residue rows introduced:

- `NumberFormatOptionsSignDisplay`
- `NumberFormatOptionsSignDisplayRegistry`

## Decision

Keep this narrow Intl options family admission. It materially reduces
declaration-file delegate residue with unchanged diagnostics while preserving
fallback behavior for utility aliases and unresolved Intl remainder (`Locale`,
`Iterator`).
