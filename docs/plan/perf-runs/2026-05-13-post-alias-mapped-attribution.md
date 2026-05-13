# 2026-05-13 — Post-#6449 Attribution Refresh

Attribution-mode refresh after the mapped-declaration alias slice
(`#6449`) removes the remaining declaration-file `DelegateCrossArenaSymbol`
residue on scale-cliff.

## Reproducer

| Item | Value |
| --- | --- |
| `tsz` commit | `e1700b2f0d` on this docs branch; code includes `perf(checker): admit mapped-decl actual-lib aliases directly` (`80765edb3c`) |
| `tsz` build | `cargo build -p tsz-cli --release --features perf-tools` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-{001..006}` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Machine | macOS Darwin 25.1.0 arm64 |
| `large-ts-repo` | still deferred (no local run in this slice) |

Raw JSON is checked in under:

- `docs/plan/perf-runs/raw/2026-05-13-post-alias-mapped-monorepo-001-{diag,pc}.json`
- `docs/plan/perf-runs/raw/2026-05-13-post-alias-mapped-monorepo-002-{diag,pc}.json`
- `docs/plan/perf-runs/raw/2026-05-13-post-alias-mapped-monorepo-003-{diag,pc}.json`
- `docs/plan/perf-runs/raw/2026-05-13-post-alias-mapped-monorepo-004-{diag,pc}.json`
- `docs/plan/perf-runs/raw/2026-05-13-post-alias-mapped-monorepo-005-{diag,pc}.json`
- `docs/plan/perf-runs/raw/2026-05-13-post-alias-mapped-monorepo-006-{diag,pc}.json`

Fixtures intentionally emit diagnostics, so `tsz` exits with code `2`. The JSON
artifacts above are the source of truth.

## Phase Split

Attribution-mode wall time is not comparable to timing-mode `tsz`/`tsgo`. Use
this only for phase dominance.

| Fixture | root files | total s | check s | parse/bind s | check % |
| --- | ---: | ---: | ---: | ---: | ---: |
| monorepo-001 | 101 | 0.08 | 0.04 | 0.02 | 56.8 |
| monorepo-002 | 1,010 | 2.52 | 2.23 | 0.20 | 88.4 |
| monorepo-003 | 5,099 | 71.55 | 70.02 | 1.12 | 97.9 |
| monorepo-004 | 5,151 | 74.27 | 72.68 | 1.22 | 97.8 |
| monorepo-005 | 5,201 | 68.80 | 67.19 | 1.17 | 97.7 |
| monorepo-006 | 5,250 | 71.70 | 70.16 | 1.12 | 97.8 |

Scale-cliff remains checker-dominated at larger fixture sizes.

## Delegate And Child-Checker Signal

| Fixture | delegate calls | lib hits | cross-file hits | misses | with parent cache | `DelegateCrossArenaSymbol` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-001 | 64 | 26 | 0 | 0 | 0 | 0 |
| monorepo-002 | 64 | 26 | 0 | 0 | 0 | 0 |
| monorepo-003 | 407 | 26 | 0 | 0 | 0 | 0 |
| monorepo-004 | 507 | 26 | 98 | 0 | 0 | 0 |
| monorepo-005 | 507 | 26 | 98 | 0 | 0 | 0 |
| monorepo-006 | 997 | 26 | 434 | 0 | 0 | 0 |

This run removes child-checker construction from all tracked
`with_parent_cache_by_reason` categories on these fixtures.

## Other Signals

On monorepo-006:

- `checker.state_constructed = 5,251` (root checker constructions only),
- `compute_type_of_symbol_calls = 26,370`,
- `compute_type_of_symbol_cache_hits = 252,026`,
- diagnostics unchanged (`10,198`).

## Decision

1. Treat T2.2 declaration-file symbol-arena residue elimination as complete for
   scale-cliff (`DelegateCrossArenaSymbol = 0`, `delegate.misses = 0`).
2. Retire the stale “next child-checker reason” assumption from T2.1.D on this
   fixture set; no `with_parent_cache` reason remains to target here.
3. Keep resolver/interner redesign deferred: this attribution refresh still
   shows checker phase dominance, but now without child-checker delegation
   misses.
4. Next measured lane should use timing-mode runs and checker-internal
   non-child-checker hotspots (for example `compute_type_of_symbol` call volume)
   rather than more `DelegateCrossArenaSymbol` slices.
