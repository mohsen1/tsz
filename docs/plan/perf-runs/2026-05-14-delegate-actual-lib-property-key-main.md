# 2026-05-14 - DelegateCrossArenaSymbol PropertyKey Alias Follow-up (Main-Based)

Attribution-mode follow-up on top of `origin/main` commit `d59f4685c3`.

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `d59f4685c3` (`origin/main`) |
| after branch | `codex/perf-record-main-20260514` (this slice) |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| fixture path | `/private/tmp/tsz-bench-fixtures/monorepo-006` |
| counter mode | `TSZ_PERF_COUNTERS=1` |
| machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-property-key-main-baseline-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-property-key-main-baseline-monorepo-006-pc.json`
- After:
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-property-key-main-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-property-key-main-after-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

## Change

Admit `PropertyKey` in the direct actual-lib alias-body allowlist.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` children | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 25 | 22 | 977 | 1 | 434 | 24 |
| monorepo-006 after | 24 | 21 | 977 | 1 | 434 | 23 |
| delta | -1 | -1 | 0 | 0 | 0 | -1 |

Diagnostics count is unchanged (`10,198` on both runs).

## Miss Classification

| Bucket | Baseline | After |
| --- | ---: | ---: |
| `target_declaration_files` | 22 | 21 |
| `target_source_files` | 0 | 0 |
| `by_kind.type_alias` | 13 | 12 |
| `by_kind.interface` | 9 | 9 |

Declaration-file residue row removed:

- `PropertyKey`

Alias-body outcomes shift:

- `success`: `3 -> 4`
- `name_not_admitted`: `1 -> 0`
- `generic_alias`: unchanged at `7`
- `missing_resolver_type`: unchanged at `5`

## Decision

Keep this narrow alias follow-up. It removes one declaration-file alias miss
on latest `main` with unchanged diagnostics.
