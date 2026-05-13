# 2026-05-13 — Post-#5863 Attribution Refresh

Follow-up attribution-mode run after #5863 added
`cross_file_cache_miss_causes` to `PerfCounterSnapshot` JSON.

## Reproducer

| Item | Value |
| --- | --- |
| `tsz` commit | `d6ae8057e0ef` on `codex/perf-post-5863-attribution-20260513`; code matches `origin/main` `c6c5f930767e` plus a claim doc |
| `tsz` build | `CARGO_TARGET_DIR=/tmp/tsz-perf-tools-target cargo build -p tsz-cli --bin tsz --features perf-tools --release` |
| Fixture generator | `scripts/bench/scale-cliff/generate-fixtures.sh --clean` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-{001..006}` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Timing feature | `tsz-common/perf-counters-timing` ON |
| Machine | macOS Darwin 25.1.0 arm64 |
| `large-ts-repo` | not re-run; plan still defers it until the first T2.2 code PR removes a measured child-checker path |

Raw JSON is checked in under
`docs/plan/perf-runs/raw/2026-05-13-monorepo-{001..006}-{diag,pc}.json`.

The synthetic fixtures still emit TypeScript diagnostics, so `tsz` exits
non-zero. The diagnostics and perf-counter JSON files are still written and are
the artifact used below.

## Phase Split

Attribution-mode wall time is not comparable to `tsgo` or timing-mode `tsz`.
Use these numbers only for phase dominance.

| Fixture | root files | total s | check s | parse/bind s | check % |
| --- | ---: | ---: | ---: | ---: | ---: |
| monorepo-001 | 101 | 0.08 | 0.05 | 0.02 | 61.9 |
| monorepo-002 | 1,010 | 2.61 | 2.34 | 0.20 | 89.6 |
| monorepo-003 | 5,099 | 74.81 | 73.31 | 1.12 | 98.0 |
| monorepo-004 | 5,151 | 74.23 | 72.76 | 1.10 | 98.0 |
| monorepo-005 | 5,201 | 76.16 | 74.69 | 1.10 | 98.1 |
| monorepo-006 | 5,250 | 80.72 | 79.20 | 1.16 | 98.1 |

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

`delegate.cache_hits_cross_file` is still zero. The cross-file type-params
cache is also still zero-hit.

## Miss-Cause Result

The new #5863 buckets are present in JSON, but all four buckets are zero on
every fixture:

| Fixture | gate off | bucket empty | sentinel error/unknown | type id not interned |
| --- | ---: | ---: | ---: | ---: |
| monorepo-001 | 0 | 0 | 0 | 0 |
| monorepo-002 | 0 | 0 | 0 | 0 |
| monorepo-003 | 0 | 0 | 0 | 0 |
| monorepo-004 | 0 | 0 | 0 | 0 |
| monorepo-005 | 0 | 0 | 0 | 0 |
| monorepo-006 | 0 | 0 | 0 | 0 |

This does **not** mean the cache is healthy. It means the hot delegate path is
not reaching the four `cached_cross_file_*` readers that #5863 instruments.
On `monorepo-006`, the load-bearing misses are:

| Signal | Count |
| --- | ---: |
| `DelegateCrossArenaSymbol` child checkers | 924 |
| miss source: `symbol_arenas` | 924 |
| target source files | 883 |
| target declaration files | 41 |
| direct interface lowering: `rejected_non_direct_arena` | 924 |

The `DelegateCrossArenaSymbol` path only probes
`cached_cross_file_symbol_type` when `needs_cross_file_delegation` is true,
which is derived from `resolve_symbol_file_index`. The measured misses are
coming from `binder.symbol_arenas` instead, so they bypass the canonical reader
and later write only to the lib delegation cache. That explains both facts:
zero cross-file hits and zero miss-cause observations.

## Child-Checker Construction

| Fixture | state constructed | with parent cache | `DelegateCrossArenaSymbol` | `TypeEnvironmentCore` | `CallHelpers` |
| --- | ---: | ---: | ---: | ---: | ---: |
| monorepo-001 | 102 | 165 | 41 | 110 | 14 |
| monorepo-002 | 1,011 | 1,074 | 41 | 1,019 | 14 |
| monorepo-003 | 5,100 | 5,506 | 384 | 5,108 | 14 |
| monorepo-004 | 5,152 | 5,608 | 434 | 5,160 | 14 |
| monorepo-005 | 5,202 | 5,658 | 434 | 5,210 | 14 |
| monorepo-006 | 5,251 | 6,197 | 924 | 5,259 | 14 |

The next T2.2 PR should target `DelegateCrossArenaSymbol` first. The concrete
target is the symbol-arena-sourced source-file path, not the already-instrumented
cache-reader miss branches.

## Decision

T2.2 stays the highest-priority performance work, but the next code PR should
not start by changing the cache key or `TypeId` validation based on empty
miss-cause buckets. It should first make symbol-arena-sourced source-file
delegations use the same canonical cross-file query bucket as
`resolve_symbol_file_index` delegations, or add one more focused counter if the
reviewed code path needs a smaller preparatory slice.

Expected success criteria for that PR:

- `DelegateCrossArenaSymbol` construction count drops on `monorepo-006`.
- `delegate.cache_hits_cross_file` becomes non-zero, or
  `cross_file_cache_miss_causes` becomes non-zero and identifies the next
  structural blocker.
- Diagnostics remain unchanged.
- Child-checker fallback remains for unsupported cases.
