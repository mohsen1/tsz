# 2026-05-13 - DelegateCrossArenaSymbol Intl Sign-Display Registry Follow-up

Attribution-mode follow-up on top of
`64573ca6c0` (`perf(checker): admit direct actual-lib Intl options interfaces`).

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `64573ca6c0` |
| after branch | `codex/perf-goal-next-20260513-replay` (this slice) |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| fixture path | `/private/tmp/tsz-bench-fixtures/monorepo-006` |
| counter mode | `TSZ_PERF_COUNTERS=1` |
| machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-13-delegate-actual-lib-intl-options-value-merged-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-delegate-actual-lib-intl-options-value-merged-after-monorepo-006-pc.json`
- After:
  `docs/plan/perf-runs/raw/2026-05-13-delegate-actual-lib-intl-sign-display-registry-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-delegate-actual-lib-intl-sign-display-registry-after-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

## Change

Extend the same narrow Intl namespace-qualified interface family to include:

- `NumberFormatOptionsSignDisplayRegistry`

No alias behavior changes; type-alias rows stay on fallback.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` children | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 18 | 18 | 976 | 1 | 434 | 18 |
| monorepo-006 after | 17 | 17 | 976 | 1 | 434 | 17 |
| delta | -1 | -1 | 0 | 0 | 0 | -1 |

Diagnostics count is unchanged (`10,198` on both runs).

## Miss Classification

| Bucket | Baseline | After |
| --- | ---: | ---: |
| `target_declaration_files` | 18 | 17 |
| `target_source_files` | 0 | 0 |
| `by_kind.type_alias` | 15 | 15 |
| `by_kind.interface` | 3 | 2 |

Declaration-file residue row removed:

- `NumberFormatOptionsSignDisplayRegistry`

## Decision

Keep this focused follow-up. It preserves diagnostics and removes one more
declaration-file interface delegate without broadening alias admission.
