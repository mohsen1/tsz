# 2026-05-14 - DelegateCrossArenaSymbol FlatArray Alias Follow-up

Attribution-mode follow-up on top of
`c924202464` (`perf(checker): admit actual-lib Partial alias`).

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `c924202464` |
| after branch | `codex/perf-actual-lib-flatarray-alias-20260514` |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| fixture path | `/private/tmp/tsz-bench-fixtures/monorepo-006` |
| counter mode | `TSZ_PERF_COUNTERS=1` |
| machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-partial-after-b-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-partial-after-b-monorepo-006-pc.json`
- After:
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-flatarray-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-flatarray-after-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

## Change

Admit `FlatArray` in the existing direct actual-lib alias-body allowlist.

`FlatArray` remains on the same proven alias-body path used by the current
mapped utility aliases, and the direct alias-body proof still matches the
existing child-checker fallback body/parameter arity.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` children | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 4 | 4 | 975 | 0 | 434 | 4 |
| monorepo-006 after | 2 | 2 | 975 | 0 | 434 | 2 |
| delta | -2 | -2 | 0 | 0 | 0 | -2 |

Diagnostics count is unchanged (`10,198` on both runs).

## Alias Outcome Shift

| Outcome | Baseline | After |
| --- | ---: | ---: |
| `success` | 13 | 15 |
| `generic_alias` | 4 | 2 |

## Miss Residues

Baseline declaration-file residue rows:

- `FlatArray` (`2`)
- `IteratorResult` (`2`)

After declaration-file residue rows:

- `IteratorResult` (`2`)

Removed row: `FlatArray`.

## Decision

Keep this narrow alias follow-up. It removes two declaration-file alias misses
without diagnostic drift.
