# 2026-05-14 - DelegateCrossArenaSymbol Iterator Proof-Bypass Follow-up (Main-Based)

Attribution-mode follow-up on top of `origin/main` commit `75c7a203d7`.

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `75c7a203d7` (`origin/main`) |
| after branch | `codex/perf-partial-main-20260514` (this slice) |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| fixture path | `/private/tmp/tsz-bench-fixtures/monorepo-006` |
| counter mode | `TSZ_PERF_COUNTERS=1` |
| machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-iterator-proof-bypass-main-baseline-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-iterator-proof-bypass-main-baseline-monorepo-006-pc.json`
- After:
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-iterator-proof-bypass-main-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-iterator-proof-bypass-main-after-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

## Change

Narrow direct actual-lib interface follow-up for iterator-family residue:

- admit `Iterator` / `IteratorObject` in the `resolve_lib_type_with_params` direct path,
- allow an `Iterator` fallback to `resolve_lib_interface_type_by_symbol` when
  parameterized lib lookup returns `None`,
- bypass strict declaration-arena proof for `Iterator` under existing actual-lib
  provenance checks,
- admit value-merged `Iterator` / `IteratorObject` in the value-interface gate.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` children | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 25 | 22 | 977 | 1 | 434 | 24 |
| monorepo-006 after | 19 | 19 | 973 | 0 | 434 | 19 |
| delta | -6 | -3 | -4 | -1 | 0 | -5 |

Diagnostics count is unchanged (`10,198` on both runs).

## Miss Classification

| Bucket | Baseline | After |
| --- | ---: | ---: |
| `target_declaration_files` | 22 | 19 |
| `target_source_files` | 0 | 0 |
| `by_kind.type_alias` | 13 | 13 |
| `by_kind.interface` | 9 | 6 |

Declaration-file residue rows removed:

- `Iterator`
- `IteratorObject`
- `Symbol`

## Decision

Keep this iterator follow-up. On latest `main` it removes three declaration-file
interface residues and five total misses with unchanged diagnostics.
