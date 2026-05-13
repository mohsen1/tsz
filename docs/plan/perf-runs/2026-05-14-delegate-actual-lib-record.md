# 2026-05-14 - DelegateCrossArenaSymbol Record Alias Follow-up

Attribution-mode follow-up on top of
`cb0246042d` (`perf(checker): admit PropertyKey direct alias body`).

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `cb0246042d` |
| after branch | `codex/perf-goal-next-20260514` (this slice) |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| fixture path | `/private/tmp/tsz-bench-fixtures/monorepo-006` |
| counter mode | `TSZ_PERF_COUNTERS=1` |
| machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-property-key-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-property-key-after-monorepo-006-pc.json`
- After:
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-record-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-record-after-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

## Change

Admit `Record<K, T>` in the existing direct actual-lib alias-body allowlist.

`Record` remains routed through the proven actual-lib alias-body path with
its declared generic metadata preserved in the delegation cache.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` children | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 13 | 13 | 975 | 0 | 434 | 13 |
| monorepo-006 after | 11 | 11 | 975 | 0 | 434 | 11 |
| delta | -2 | -2 | 0 | 0 | 0 | -2 |

Diagnostics count is unchanged (`10,198` on both runs).

## Miss Classification

| Bucket | Baseline | After |
| --- | ---: | ---: |
| `target_declaration_files` | 13 | 11 |
| `target_source_files` | 0 | 0 |
| `by_kind.type_alias` | 13 | 11 |
| `by_kind.interface` | 0 | 0 |

Declaration-file residue row removed:

- `Record` (count `2`)

Alias-body outcomes shift:

- `success`: `4 -> 6`
- `generic_alias`: `7 -> 5`
- `missing_resolver_type`: unchanged at `6`

## Decision

Keep this alias follow-up. It removes two declaration-file alias misses with
unchanged diagnostics and keeps `Partial`/`FlatArray`/`IteratorResult` on
fallback pending broader generic-alias proof.
