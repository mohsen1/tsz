# 2026-05-14 - DelegateCrossArenaSymbol PropertyKey Alias Follow-up

Attribution-mode follow-up on top of
`0d22de5200` (`perf(checker): bypass iterator declaration-proof gate`).

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `0d22de5200` |
| after branch | `codex/perf-goal-next-20260514` (this slice) |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| fixture path | `/private/tmp/tsz-bench-fixtures/monorepo-006` |
| counter mode | `TSZ_PERF_COUNTERS=1` |
| machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-iterator-proof-bypass-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-iterator-proof-bypass-after-monorepo-006-pc.json`
- After:
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-property-key-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-property-key-after-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

## Change

Admit `PropertyKey` in the existing direct actual-lib alias-body allowlist.

`PropertyKey` remains non-generic and still routes through the same alias-body
proof and cache plumbing already used by other admitted aliases.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` children | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 14 | 14 | 975 | 0 | 434 | 14 |
| monorepo-006 after | 13 | 13 | 975 | 0 | 434 | 13 |
| delta | -1 | -1 | 0 | 0 | 0 | -1 |

Diagnostics count is unchanged (`10,198` on both runs).

## Miss Classification

| Bucket | Baseline | After |
| --- | ---: | ---: |
| `target_declaration_files` | 14 | 13 |
| `target_source_files` | 0 | 0 |
| `by_kind.type_alias` | 14 | 13 |
| `by_kind.interface` | 0 | 0 |

Declaration-file residue row removed:

- `PropertyKey`

## Decision

Keep this narrow alias follow-up. It removes one declaration-file alias miss
without diagnostic drift.
