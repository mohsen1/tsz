# 2026-05-14 - DelegateCrossArenaSymbol IteratorResult Alias Follow-up

Attribution-mode follow-up on top of `origin/main` at `20eaab2634`
(`chore(dts): split return type normalization helpers`).

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `20eaab2634` |
| after branch | `codex/perf-actual-lib-iterator-result-alias-20260514` |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| fixture provenance | regenerated with `scripts/bench/scale-cliff/generate-fixtures.sh` |
| counter mode | `TSZ_PERF_COUNTERS=1` |
| machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-iterator-result-baseline-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-iterator-result-baseline-monorepo-006-pc.json`
- After:
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-iterator-result-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-iterator-result-after-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits non-zero.
Artifacts are still written and are the source of truth.

## Change

Admit `IteratorResult` in the existing direct actual-lib alias-body allowlist.

`IteratorResult` uses the same proof-backed alias-body path as the already
admitted mapped utility aliases, and the proof test now asserts that the direct
body matches the fallback body while preserving both declared type parameters.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` children | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 4 | 4 | 977 | 0 | 434 | 4 |
| monorepo-006 after | 2 | 2 | 977 | 0 | 434 | 2 |
| delta | -2 | -2 | 0 | 0 | 0 | -2 |

Diagnostics count is unchanged (`10,198` on both runs).

## Alias Outcome Shift

| Outcome | Baseline | After |
| --- | ---: | ---: |
| `success` | 15 | 17 |
| `generic_alias` | 2 | 0 |

## Miss Residues

Baseline declaration-file residue rows:

- `IteratorResult` (`2`)
- `TextInfo` (`1`)
- `WeekInfo` (`1`)

After declaration-file residue rows:

- `TextInfo` (`1`)
- `WeekInfo` (`1`)

Removed row: `IteratorResult`.

## Decision

Keep this narrow alias follow-up. It removes two declaration-file alias misses
without diagnostic drift and keeps the known `IteratorResult` assignability
pollution regressions covered by focused tests.
