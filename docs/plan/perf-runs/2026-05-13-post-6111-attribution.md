# 2026-05-13 — Post-#6111 Landing Attribution Refresh

Attribution-mode refresh after #6111 landed on `origin/main`. This is the
actual post-merge run requested by the earlier afternoon record
[`2026-05-13-attribution-post-6111.md`](2026-05-13-attribution-post-6111.md),
whose measurements were taken before #6111 was merged.

## Reproducer

| Item | Value |
| --- | --- |
| `tsz` commit | `368ca851f5bd` on this docs branch; code matches `origin/main` `429d15f520a2` plus the initial draft record |
| `tsz` build | `CARGO_TARGET_DIR=.target cargo build -p tsz-cli --bin tsz --features perf-tools --release` |
| Fixture generator | `scripts/bench/scale-cliff/generate-fixtures.sh --clean` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-{001..006}` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Timing feature | `tsz-common/perf-counters-timing` ON via `perf-tools` |
| Machine | macOS Darwin 25.1.0 arm64 |
| `large-ts-repo` | not re-run; still deferred until child-checker recursion drops further |

Raw JSON is checked in under
`docs/plan/perf-runs/raw/2026-05-13-post-6111-monorepo-{001..006}-{diag,pc}.json`.

The synthetic fixtures still emit diagnostics, so `tsz` exits with code `2`.
The diagnostics and perf-counter JSON files are still written and are the
artifacts used below.

## Phase Split

Attribution-mode wall time is not comparable to `tsgo` or timing-mode `tsz`.
Use these numbers only for phase dominance.

| Fixture | root files | total s | check s | parse/bind s | check % |
| --- | ---: | ---: | ---: | ---: | ---: |
| monorepo-001 | 101 | 0.11 | 0.06 | 0.03 | 55.1 |
| monorepo-002 | 1,010 | 2.87 | 2.54 | 0.25 | 88.4 |
| monorepo-003 | 5,099 | 74.79 | 73.02 | 1.40 | 97.6 |
| monorepo-004 | 5,151 | 80.02 | 78.12 | 1.53 | 97.6 |
| monorepo-005 | 5,201 | 82.58 | 80.17 | 1.93 | 97.1 |
| monorepo-006 | 5,250 | 91.10 | 89.11 | 1.61 | 97.8 |

The cliff remains checker-dominated. Resolver/source discovery stays deferred.

## Delegate Cache Signal

| Fixture | delegate calls | lib hits | cross-file hits | misses | type-param hits | type-param misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-001 | 58 | 3 | 0 | 55 | 0 | 110 |
| monorepo-002 | 58 | 3 | 0 | 55 | 0 | 1,019 |
| monorepo-003 | 401 | 3 | 0 | 398 | 0 | 5,108 |
| monorepo-004 | 451 | 3 | 0 | 448 | 0 | 5,160 |
| monorepo-005 | 451 | 3 | 0 | 448 | 0 | 5,210 |
| monorepo-006 | 941 | 3 | 0 | 938 | 0 | 5,259 |

#6111 did move source-file symbol-arena lookups onto the canonical reader path:
`cross_file_cache_miss_causes.bucket_empty` is now non-zero. The final landed
requester-scoped cache key does not produce reusable batch hits on this run,
so `delegate.cache_hits_cross_file` remains zero.

## Miss Causes

| Fixture | gate off | bucket empty | sentinel error/unknown | type id not interned |
| --- | ---: | ---: | ---: | ---: |
| monorepo-001 | 0 | 0 | 0 | 0 |
| monorepo-002 | 0 | 0 | 0 | 0 |
| monorepo-003 | 0 | 98 | 0 | 0 |
| monorepo-004 | 0 | 98 | 0 | 0 |
| monorepo-005 | 0 | 98 | 0 | 0 |
| monorepo-006 | 0 | 343 | 0 | 0 |

This is different from the pre-#6111 attribution records where every miss-cause
bucket was zero because the hot `DelegateCrossArenaSymbol` path bypassed the
reader entirely. The post-merge residue is now observable as `bucket_empty`.

## Child-Checker Construction

| Fixture | state constructed | with parent cache | `DelegateCrossArenaSymbol` | `TypeEnvironmentCore` | `CallHelpers` |
| --- | ---: | ---: | ---: | ---: | ---: |
| monorepo-001 | 102 | 165 | 41 | 110 | 14 |
| monorepo-002 | 1,011 | 1,074 | 41 | 1,019 | 14 |
| monorepo-003 | 5,100 | 5,506 | 384 | 5,108 | 14 |
| monorepo-004 | 5,152 | 5,608 | 434 | 5,160 | 14 |
| monorepo-005 | 5,202 | 5,658 | 434 | 5,210 | 14 |
| monorepo-006 | 5,251 | 6,197 | 924 | 5,259 | 14 |

`TypeEnvironmentCore` remains the largest child-checker reason by a wide
margin: 5,259 on monorepo-006 versus 924 for `DelegateCrossArenaSymbol`.
That confirms the lane already claimed by #6144 is still the next largest
T2.2/T2.1.D lever.

## Other Signals

On monorepo-006:

- resolver probes are not hot: `is_file_calls = 1`, `is_dir_calls = 1`,
  `read_dir_calls = 0`, `package_json_reads = 102`.
- interner lock-wait histogram remains cold: `[179287, 366, 2, 0, 1, 0, 0, 0]`,
  so T2.4 stays de-prioritised.
- `delegate_miss_classification` still reports 924 `symbol_arenas` misses:
  16 type aliases, 368 interfaces, 540 variables; 883 source-file targets and
  41 declaration-file targets.

## Decision

1. Keep T2.2/T2.1.D focused on `TypeEnvironmentCore` next. The open #6144 lane
   is still the right next implementation target.
2. Treat #6111's landed cache path as instrumentation progress for batch mode,
   not as a batch-speed win: it converts previously invisible source-file
   symbol-arena misses into `bucket_empty`, but does not create repeat hits in
   this compile.
3. After the TypeEnvironmentCore arena-direct slice lands, re-run monorepo-006
   first. If `TypeEnvironmentCore` drops materially, revisit the remaining
   `DelegateCrossArenaSymbol` residue with the now-observable `bucket_empty`
   data.

