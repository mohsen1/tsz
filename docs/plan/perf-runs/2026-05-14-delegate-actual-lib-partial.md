# 2026-05-14 - DelegateCrossArenaSymbol Partial Alias Follow-up

Attribution-mode follow-up on top of `origin/main` at `456f2d1a8e`
(`fix(checker): suppress TS2488 for generic deferred spread types`).

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `456f2d1a8e` |
| after branch | `codex/perf-actual-lib-partial-alias-20260514b` |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| fixture path | `/private/tmp/tsz-bench-fixtures/monorepo-006` |
| counter mode | `TSZ_PERF_COUNTERS=1` |
| machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-partial-baseline-b-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-partial-baseline-b-monorepo-006-pc.json`
- After:
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-partial-after-b-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-partial-after-b-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

## Change

Admit `Partial` in the existing direct actual-lib alias-body allowlist.

`Partial` still goes through the same proven alias-body path and still records
its generic type-parameter metadata (`T`) in the direct delegation cache.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` children | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 5 | 5 | 975 | 0 | 434 | 5 |
| monorepo-006 after | 4 | 4 | 975 | 0 | 434 | 4 |
| delta | -1 | -1 | 0 | 0 | 0 | -1 |

Diagnostics count is unchanged (`10,198` on both runs).

## Alias Outcome Shift

| Outcome | Baseline | After |
| --- | ---: | ---: |
| `success` | 12 | 13 |
| `generic_alias` | 5 | 4 |

## Miss Residues

Baseline declaration-file residue rows:

- `FlatArray` (`2`)
- `IteratorResult` (`2`)
- `Partial` (`1`)

After declaration-file residue rows:

- `FlatArray` (`2`)
- `IteratorResult` (`2`)

Removed row: `Partial`.

## Decision

Keep this narrow alias follow-up. It removes one declaration-file alias miss
without diagnostic drift.
