# 2026-05-14 - DelegateCrossArenaSymbol Iterator Declaration-Proof Bypass Follow-up

Attribution-mode follow-up on top of
`71944a9280` (`perf(checker): allow direct Intl.Locale heritage lowering`).

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `71944a9280` |
| after branch | `codex/perf-goal-next-20260514` (this slice) |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| fixture path | `/private/tmp/tsz-bench-fixtures/monorepo-006` |
| counter mode | `TSZ_PERF_COUNTERS=1` |
| machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-13-delegate-actual-lib-locale-heritage-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-delegate-actual-lib-locale-heritage-after-monorepo-006-pc.json`
- After:
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-iterator-proof-bypass-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-iterator-proof-bypass-after-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

## Change

Keep the same direct actual-lib path, but add a narrow declaration-proof bypass:

- When symbol provenance is already proven actual/cloned bundled-lib and the
  delegate arena is still a non-DOM bundled-lib declaration arena, allow
  `Iterator` to continue through direct lowering even if
  `symbol_declarations_are_direct_actual_lib_only` fails.

No alias-admission broadening was added.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` children | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 16 | 16 | 976 | 1 | 434 | 16 |
| monorepo-006 after | 14 | 14 | 975 | 0 | 434 | 14 |
| delta | -2 | -2 | -1 | -1 | 0 | -2 |

Diagnostics count is unchanged (`10,198` on both runs).

## Miss Classification

| Bucket | Baseline | After |
| --- | ---: | ---: |
| `target_declaration_files` | 16 | 14 |
| `target_source_files` | 0 | 0 |
| `by_kind.type_alias` | 15 | 14 |
| `by_kind.interface` | 1 | 0 |

Declaration-file residue rows removed:

- `Iterator` (`interface`)
- `Readonly` (`type_alias`)

Remaining residue rows are all type aliases.

## Decision

Keep this narrow follow-up. It removes the final declaration-file interface
residue and lowers total declaration-file misses without widening generic alias
admission.
