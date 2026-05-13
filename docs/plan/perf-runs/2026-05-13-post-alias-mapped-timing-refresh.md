# 2026-05-13 — Post-#6449 Timing-Mode Refresh And Hotspot Selection

Timing-mode refresh after the post-#6449 attribution closure. This run answers
`PERFORMANCE_PLAN.md` status-table next action: refresh timing-mode numbers and
select a non-child-checker checker hotspot.

## Reproducer

| Item | Value |
| --- | --- |
| `tsz` commit | `e2c9a0b039` (post-#6454 docs stack base, no checker code changes in this slice) |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --release` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-{001..006}` |
| Counter mode | off (`TSZ_PERF_COUNTERS` unset) |
| Command | `/usr/bin/time -l .target/release/tsz --noEmit -p <fixture>/tsconfig.json --extendedDiagnostics --pretty false` |
| Notes | `tsz` exits with code `2` because fixtures intentionally emit diagnostics |

Raw artifacts are checked in under:

- `docs/plan/perf-runs/raw/2026-05-13-timing-post-alias-mapped-plain-runs.csv`
- `docs/plan/perf-runs/raw/2026-05-13-timing-post-alias-mapped-plain-runs.json`
- `docs/plan/perf-runs/raw/2026-05-13-timing-post-alias-mapped-plain-summary.csv`
- `docs/plan/perf-runs/raw/2026-05-13-timing-post-alias-mapped-plain-summary.json`

The run-level artifacts above are parsed directly from the full command output
for each fixture/run pair and retain all numbers used in this record.

## Timing Summary (Median Over Available Runs)

For monorepo-003..006, two timing runs were captured and summarized with median
values to reduce single-run noise. monorepo-001..002 have one run each.

| Fixture | runs | files | total s (median) | check s (median) | parse/bind s (median) | I/O read s (median) | check % (median) | diagnostics |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-001 | 1 | 183 | 0.11 | 0.05 | 0.03 | 0.02 | 45.5 | 199 |
| monorepo-002 | 1 | 1,092 | 4.88 | 4.47 | 0.21 | 0.15 | 91.6 | 1,990 |
| monorepo-003 | 2 | 5,181 | 82.31 | 79.60 | 1.87 | 0.60 | 96.7 | 9,999 |
| monorepo-004 | 2 | 5,233 | 76.16 | 74.09 | 1.43 | 0.47 | 97.3 | 10,050 |
| monorepo-005 | 2 | 5,283 | 79.59 | 77.91 | 1.16 | 0.34 | 97.9 | 10,100 |
| monorepo-006 | 2 | 5,332 | 80.97 | 79.35 | 1.10 | 0.35 | 98.0 | 10,198 |

## Hotspot Selection (Post Child-Checker Elimination)

The post-#6449 attribution artifacts already show child-checker-specific
counters resolved on this fixture family:

- `delegate.misses = 0` on monorepo-001..006,
- `checker.with_parent_cache_constructed = 0` on monorepo-001..006,
- `DelegateCrossArenaSymbol = 0` on monorepo-001..006.

With child-checker residue eliminated, checker time remains dominant in
**timing mode** at the cliff (monorepo-003..006: ~96.7-98.0%).

From the latest attribution artifact (`2026-05-13-post-alias-mapped-monorepo-006-pc.json`):

- `checker.compute_type_of_symbol_calls = 26,370`
- `checker.compute_type_of_symbol_cache_hits = 252,026`
- `interner.intern_calls = 478,794` (`intern_hits = 402,055`, `intern_misses = 76,739`)

From the parsed timing artifacts (monorepo-006 rows in
`2026-05-13-timing-post-alias-mapped-plain-runs.{csv,json}`):

- `Request cache misses = 25,000`
- `Contextual cache bypasses = 50,000`

These numbers make `compute_type_of_symbol` call volume the next concrete,
checker-internal hotspot target after child-checker elimination.

## Decision

1. Mark the timing refresh requirement complete for the current scale-cliff
   fixture set (timing-mode median summary now checked in).
2. Keep resolver fast-path and interner redesign status unchanged (still
   deferred / de-prioritized by existing evidence).
3. Target the next optimization lane at `compute_type_of_symbol` volume:
   add caller attribution and then reduce redundant call patterns in the top
   buckets while preserving diagnostics.
