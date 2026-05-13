# 2026-05-13 - TypeEnvironmentCore Arena-Direct Attribution

Attribution-mode refresh for #6144 after extending the arena-direct type-param
path in `TypeEnvironmentCore`. This run sequences after the post-#6111 record
[`2026-05-13-post-6111-attribution.md`](2026-05-13-post-6111-attribution.md).

## Reproducer

| Item | Value |
| --- | --- |
| `tsz` code commit | `7dd844e57891` before docs-only updates |
| `origin/main` base | `ea032ba75bc6` |
| `tsz` build | `CARGO_TARGET_DIR=.target cargo build -p tsz-cli --bin tsz --features perf-tools --release` |
| Fixture generator | `scripts/bench/scale-cliff/generate-fixtures.sh --clean` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-{001..006}` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Timing feature | `tsz-common/perf-counters-timing` ON via `perf-tools` |
| Machine | macOS Darwin 25.1.0 arm64 |
| `large-ts-repo` | not re-run; still deferred until child-checker recursion drops further |

Raw JSON is checked in under
`docs/plan/perf-runs/raw/2026-05-13-typeenv-arena-direct-monorepo-{001..006}-{diag,pc}.json`.

The synthetic fixtures still emit diagnostics, so `tsz` exits with code `2`.
The diagnostics and perf-counter JSON files are still written and are the
artifacts used below.

## Child-Checker Construction

| Fixture | with parent cache before | with parent cache after | `TypeEnvironmentCore` before | `TypeEnvironmentCore` after | `DelegateCrossArenaSymbol` after | `CallHelpers` after |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-001 | 165 | 56 | 110 | 1 | 41 | 14 |
| monorepo-002 | 1,074 | 56 | 1,019 | 1 | 41 | 14 |
| monorepo-003 | 5,506 | 399 | 5,108 | 1 | 384 | 14 |
| monorepo-004 | 5,608 | 449 | 5,160 | 1 | 434 | 14 |
| monorepo-005 | 5,658 | 449 | 5,210 | 1 | 434 | 14 |
| monorepo-006 | 6,197 | 939 | 5,259 | 1 | 924 | 14 |

The `with_parent_cache_constructed` delta matches the removed
`TypeEnvironmentCore` constructions exactly. `DelegateCrossArenaSymbol` remains
unchanged, which confirms this slice did not accidentally move the #6111 lane.

## TypeEnvironmentCore Delta

| Fixture | before | after | delta |
| --- | ---: | ---: | ---: |
| monorepo-001 | 110 | 1 | -109 |
| monorepo-002 | 1,019 | 1 | -1,018 |
| monorepo-003 | 5,108 | 1 | -5,107 |
| monorepo-004 | 5,160 | 1 | -5,159 |
| monorepo-005 | 5,210 | 1 | -5,209 |
| monorepo-006 | 5,259 | 1 | -5,258 |

## Phase Split

Attribution-mode wall time is not comparable to `tsgo` or timing-mode `tsz`.
Use these numbers only for phase dominance.

| Fixture | total s | check s | check % |
| --- | ---: | ---: | ---: |
| monorepo-001 | 0.10 | 0.06 | 59.2 |
| monorepo-002 | 3.07 | 2.71 | 88.5 |
| monorepo-003 | 73.78 | 71.83 | 97.4 |
| monorepo-004 | 75.16 | 73.21 | 97.4 |
| monorepo-005 | 77.63 | 75.78 | 97.6 |
| monorepo-006 | 84.01 | 82.07 | 97.7 |

The cliff remains checker-dominated after removing this construction path.

## Delegate Cache Signal

| Fixture | delegate calls | lib hits | cross-file hits | misses | type-param hits | type-param misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-001 | 58 | 3 | 0 | 55 | 0 | 1 |
| monorepo-002 | 58 | 3 | 0 | 55 | 0 | 1 |
| monorepo-003 | 401 | 3 | 0 | 398 | 0 | 1 |
| monorepo-004 | 451 | 3 | 0 | 448 | 0 | 1 |
| monorepo-005 | 451 | 3 | 0 | 448 | 0 | 1 |
| monorepo-006 | 941 | 3 | 0 | 938 | 0 | 1 |

The remaining type-param miss is the unsupported semantic case that still
requires slow-path lowering. It is no longer scale-proportional.

## Decision

1. Treat #6144 as a successful T2.2/T2.1.D slice: it removes the dominant
   `TypeEnvironmentCore` child-checker construction path on the scale-cliff
   fixtures.
2. Keep `large-ts-repo` deferred until one more measured child-checker path is
   removed or until stack behavior is re-audited.
3. Next performance work should return to the remaining
   `DelegateCrossArenaSymbol` residue and the observable
   `cross_file_cache_miss_causes.bucket_empty` signal.
