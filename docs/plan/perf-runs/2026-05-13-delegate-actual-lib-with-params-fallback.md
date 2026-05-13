# 2026-05-13 - DelegateCrossArenaSymbol Actual-Lib `resolve_lib_type_with_params` Fallback Slice

Attribution-mode follow-up on top of #6354 (`core value-interface allowlist`).
This slice keeps the same direct actual-lib safety gates, but adds a targeted
`resolve_lib_type_with_params` path for unresolved interface names where
`resolve_lib_type_by_name` currently returns `None`.

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `094abc38d3` (`docs(perf): refresh core value allowlist numbers after rebase`) |
| `tsz` build (baseline) | `CARGO_TARGET_DIR=/tmp/tsz-target-next-base cargo build -p tsz-cli --bin tsz --features perf-tools --release` |
| `tsz` build (after) | `CARGO_TARGET_DIR=/tmp/tsz-target-next-after cargo build -p tsz-cli --bin tsz --features perf-tools --release` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-with-params-fallback-baseline-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-with-params-fallback-baseline-monorepo-006-pc.json`
- After:
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-with-params-fallback-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-with-params-fallback-after-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

## Change

`direct_actual_lib_symbol_type` now attempts
`resolve_lib_type_with_params` first for a narrow set of names that were
observed to miss with `resolve_lib_type_by_name` in this direct path:

- `ArrayIterator`
- `DateTimeFormatOptions`
- `Locale`
- `NumberFormatOptions`
- `NumberFormatOptionsCurrencyDisplayRegistry`
- `NumberFormatOptionsStyleRegistry`
- `NumberFormatOptionsUseGroupingRegistry`
- `Object`
- `RegExpStringIterator`
- `StringIterator`

If that path still does not resolve, the existing
`resolve_lib_type_by_name` + `Intl.CollatorOptions` fallback path is retained.
All existing direct-path gates remain unchanged.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 37 | 28 | 984 | 2 | 434 | 36 |
| monorepo-006 after | 28 | 25 | 977 | 1 | 434 | 27 |
| delta | -9 | -3 | -7 | -1 | 0 | -9 |

Diagnostics count is unchanged (`10,198` on both runs).

## Miss Classification

| Bucket | Baseline | After |
| --- | ---: | ---: |
| `target_declaration_files` | 28 | 25 |
| `target_source_files` | 0 | 0 |
| `by_kind.type_alias` | 16 | 16 |
| `by_kind.interface` | 12 | 9 |

The remaining declaration-file residue is now 25 misses: 16 type aliases plus 9
interfaces.

## Decision

1. Keep the targeted `resolve_lib_type_with_params` pre-path for this proven
   unresolved-name slice.
2. Keep the existing alias fallback path unchanged.
3. Next interface residue should focus on symbols still blocked by
   declaration-arena/name matching (`Iterator`) and value-allowlist policy
   (`Symbol`) or unresolved namespace/merged cases.
