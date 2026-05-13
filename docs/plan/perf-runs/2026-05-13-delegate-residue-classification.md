# 2026-05-13 - DelegateCrossArenaSymbol Residue Classification

Attribution-mode follow-up for #6203 after #6191 reduced the stable
source-file symbol-arena bucket-empty residue. This run adds a narrow
eligibility split for the stable source-file symbol-arena cache key.

## Reproducer

| Item | Value |
| --- | --- |
| `tsz` code commit | `27945b04f7` before docs-only updates |
| `origin/main` base | `ca60942d2cd8` |
| `tsz` build | `CARGO_TARGET_DIR=.target cargo build -p tsz-cli --bin tsz --features perf-tools --release` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Timing feature | `tsz-common/perf-counters-timing` ON via `perf-tools` |
| Machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- `docs/plan/perf-runs/raw/2026-05-13-delegate-residue-classification-monorepo-006-diag.json`
- `docs/plan/perf-runs/raw/2026-05-13-delegate-residue-classification-monorepo-006-pc.json`

The synthetic fixture still emits diagnostics, so `tsz` exits with code `2`.
The diagnostics and perf-counter JSON files are still written and are the
artifacts used below.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 | 843 | 828 | 941 | 3 | 96 | 842 |

The implementation is attribution-only, so the child-checker counts match the
post-#6191 baseline: `DelegateCrossArenaSymbol = 828`, `bucket_empty = 247`,
and `delegate.cache_hits_cross_file = 96`.

## Source-File Symbol-Arena Cache Eligibility

| Bucket | Count | Interpretation |
| --- | ---: | --- |
| `eligible` | 343 | Stable source-file key is available. This is the 96 existing hits plus 247 cold first reads. |
| `declaration_file` | 44 | Declaration-file target through the symbol-arena path. Three are lib cache hits, leaving 41 child-checker misses. |
| `unstable_symbol` | 540 | No stable source-file cache key under the current proof. These line up with the 540 variable-symbol misses. |

All other eligibility buckets are zero on this run:
`non_symbol_arena_source`, `module_augmentation`, `current_arena`,
`missing_arena`, `missing_source_file`, and `missing_file_index`.

## Residue Split

The 828 remaining `DelegateCrossArenaSymbol` child-checker constructions split
as:

| Slice | Count | Source |
| --- | ---: | --- |
| Stable source-file key, cold cache | 247 | `source_file_symbol_arena_cache_eligibility.eligible = 343` minus 96 cross-file hits, also equal to `cross_file_cache_miss_causes.bucket_empty`. |
| Source-file variable symbols outside the current stable proof | 540 | `source_file_symbol_arena_cache_eligibility.unstable_symbol`, matching `delegate_miss_classification.by_kind.variable`. |
| Declaration-file targets | 41 | `delegate_miss_classification.target_declaration_files`; `declaration_file = 44` includes the 3 lib cache hits. |

`direct_interface_lowering_outcomes.rejected_non_direct_arena = 828`, so the
existing direct interface lowering path does not cover this residue. The alias
shortcut also reports `not_alias = 828`.

## Phase Split

Attribution-mode wall time is not comparable to `tsgo` or timing-mode `tsz`.
Use these numbers only for phase dominance.

| Fixture | total s | check s | check % | diagnostics |
| --- | ---: | ---: | ---: | ---: |
| monorepo-006 | 79.22 | 77.46 | 97.8 | 10,198 |

## Decision

1. Keep #6191's program-scoped cache key unchanged. It is still paying for the
   96 repeat requesters and cleanly identifies 247 first-requester cold reads.
2. The next implementation target should be the 540 source-file variable-symbol
   misses currently rejected as `unstable_symbol`. A follow-up must prove and
   test a requester-independent variable subset before sharing results through
   the stable source-file symbol-arena key.
3. Declaration-file symbol-arena misses are lower priority at 41 constructions.
   Direct interface lowering is not the next lever because every remaining
   attempt is rejected before symbol-shape checks.
