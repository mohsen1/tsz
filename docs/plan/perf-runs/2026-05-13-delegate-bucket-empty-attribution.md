# 2026-05-13 - DelegateCrossArenaSymbol Bucket-Empty Attribution

Attribution-mode refresh for #6191 after making stable source-file
symbol-arena cache entries program-scoped instead of requester-scoped.
This sequences after #6144 and uses
[`2026-05-13-typeenv-arena-direct-attribution.md`](2026-05-13-typeenv-arena-direct-attribution.md)
as the baseline.

## Reproducer

| Item | Value |
| --- | --- |
| `tsz` code commit | `b3c4e5529f9c` before docs-only updates |
| `origin/main` base | `e8a64ad74fb5` |
| `tsz` build | `CARGO_TARGET_DIR=.target cargo build -p tsz-cli --bin tsz --features perf-tools --release` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-{001..006}` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Timing feature | `tsz-common/perf-counters-timing` ON via `perf-tools` |
| Machine | macOS Darwin 25.1.0 arm64 |
| `large-ts-repo` | not re-run; still deferred |

After the branch was rebased onto `origin/main` `ded7a063dab4`, a
`monorepo-006` spot-check at code commit `d5ea3d36f427` reproduced the same
counter tuple used below: `with_parent_cache_constructed = 843`,
`DelegateCrossArenaSymbol = 828`, `delegate.cache_hits_cross_file = 96`,
`delegate.misses = 842`, and `bucket_empty = 247`.

Raw JSON is checked in under
`docs/plan/perf-runs/raw/2026-05-13-delegate-bucket-empty-monorepo-{001..006}-{diag,pc}.json`.

The synthetic fixtures still emit diagnostics, so `tsz` exits with code `2`.
The diagnostics and perf-counter JSON files are still written and are the
artifacts used below.

## Child-Checker Construction

| Fixture | with parent cache before | with parent cache after | `DelegateCrossArenaSymbol` before | `DelegateCrossArenaSymbol` after | delta |
| --- | ---: | ---: | ---: | ---: | ---: |
| monorepo-001 | 56 | 56 | 41 | 41 | 0 |
| monorepo-002 | 56 | 56 | 41 | 41 | 0 |
| monorepo-003 | 399 | 399 | 384 | 384 | 0 |
| monorepo-004 | 449 | 449 | 434 | 434 | 0 |
| monorepo-005 | 449 | 449 | 434 | 434 | 0 |
| monorepo-006 | 939 | 843 | 924 | 828 | -96 |

The win is isolated to `monorepo-006`, where the stable source-file
symbol-arena result is reused by later requesters. Earlier fixtures still have
no repeat requester pattern for this stable subset.

## Delegate Cache Signal

| Fixture | calls | lib hits | cross-file hits before | cross-file hits after | misses before | misses after | bucket empty before | bucket empty after |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-001 | 58 | 3 | 0 | 0 | 55 | 55 | 0 | 0 |
| monorepo-002 | 58 | 3 | 0 | 0 | 55 | 55 | 0 | 0 |
| monorepo-003 | 401 | 3 | 0 | 0 | 398 | 398 | 98 | 98 |
| monorepo-004 | 451 | 3 | 0 | 0 | 448 | 448 | 98 | 98 |
| monorepo-005 | 451 | 3 | 0 | 0 | 448 | 448 | 98 | 98 |
| monorepo-006 | 941 | 3 | 0 | 96 | 938 | 842 | 343 | 247 |

The `monorepo-006` delta converts 96 `bucket_empty` misses into real
`delegate.cache_hits_cross_file` hits. The cache still retains the per-program
scope key, so small file/symbol ids are not shared across virtual programs.

## Phase Split

Attribution-mode wall time is not comparable to `tsgo` or timing-mode `tsz`.
Use these numbers only for phase dominance.

| Fixture | total s | check s | check % |
| --- | ---: | ---: | ---: |
| monorepo-001 | 0.09 | 0.05 | 59.1 |
| monorepo-002 | 2.85 | 2.51 | 88.0 |
| monorepo-003 | 72.10 | 70.34 | 97.6 |
| monorepo-004 | 73.63 | 71.85 | 97.6 |
| monorepo-005 | 78.81 | 76.54 | 97.1 |
| monorepo-006 | 81.57 | 79.67 | 97.7 |

## Decision

1. Keep the stable source-file symbol-arena cache key program-scoped but not
   requester-scoped for the proven single-declaration class/interface subset.
2. The remaining `DelegateCrossArenaSymbol` residue is still large on
   `monorepo-006` (828 constructions). The next slice needs to classify the
   247 remaining `bucket_empty` probes plus the non-cacheable symbol-arena
   misses, especially variable and interface symbols rejected by direct
   lowering.
3. `large-ts-repo` remains deferred until one more measured child-checker path
   drops or stack behavior is re-audited.
