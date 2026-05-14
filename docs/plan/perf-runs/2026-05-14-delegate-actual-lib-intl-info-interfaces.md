# 2026-05-14 - DelegateCrossArenaSymbol Intl Info Interface Follow-up

Attribution-mode evidence captured on `origin/main` at `eb72db3709`
(`perf(checker): admit actual-lib IteratorResult alias`). The PR branch was
then rebased onto `21f0d5989c` after main moved.

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `eb72db3709` |
| after branch | `codex/perf-actual-lib-intl-info-interfaces-20260514` |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| counter mode | `TSZ_PERF_COUNTERS=1` |
| machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-iterator-result-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-iterator-result-after-monorepo-006-pc.json`
- After:
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-intl-info-interfaces-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-intl-info-interfaces-after-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits non-zero.
Artifacts are still written and are the source of truth.

## Change

Admit `Intl.TextInfo` and `Intl.WeekInfo` in the existing direct actual-lib
namespace interface path.

Both interfaces are proven through the same actual-lib declaration guards as
the earlier Intl option/registry interfaces. The direct path still requires an
actual built-in lib declaration arena, actual/cloned lib symbol provenance, and
an `Intl` namespace export whose symbol id matches the delegated symbol.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` children | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 2 | 2 | 977 | 0 | 434 | 2 |
| monorepo-006 after | 0 | 0 | 977 | 0 | 434 | 0 |
| delta | -2 | -2 | 0 | 0 | 0 | -2 |

Diagnostics count is unchanged (`10,198` on both runs).

`compute_type_of_symbol_calls` also drops by two (`26,354 -> 26,352`) because
the two residual namespace interfaces no longer construct child checkers.

## Miss Residues

Baseline declaration-file residue rows:

- `TextInfo` (`1`)
- `WeekInfo` (`1`)

After declaration-file residue rows: none.

Removed rows: `TextInfo`, `WeekInfo`.

## Decision

Keep this narrow interface follow-up. It closes the measured declaration-file
delegate tail without broadening the resolver or changing diagnostics, and it
uses the same `Intl` namespace proof boundary already used for the admitted
Intl option interfaces.
