# 2026-05-13 - DelegateCrossArenaSymbol Actual-Lib Intl Fallback Expansion

Attribution-mode follow-up on top of #6359 (`with-params fallback`).
This slice extends the direct actual-lib path for the remaining interface
residue by broadening `Intl.*` namespace fallback coverage and admitting a
small additional value/with-params subset.

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `35e1256127` (`docs(perf): record with-params fallback attribution run`) |
| `tsz` build (after) | `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo build -p tsz-cli --bin tsz --features perf-tools --release` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-intl-fallback-expansion-baseline-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-intl-fallback-expansion-baseline-monorepo-006-pc.json`
- After:
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-intl-fallback-expansion-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-intl-fallback-expansion-after-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

## Change

`direct_actual_lib_symbol_type` keeps existing safety gates but expands the
last-mile direct resolution path:

1. expands the `Intl.*` namespace fallback allowlist beyond
   `CollatorOptions` to include
   `DateTimeFormatOptions`, `Locale`, `NumberFormatOptions`, and the
   `NumberFormatOptions*Registry` interfaces,
2. allows `Symbol` in the narrow value-bearing direct-actual-lib value
   allowlist,
3. adds `IteratorObject` to the targeted
   `resolve_lib_type_with_params`-first set.

If the with-params path still misses, the existing
`resolve_lib_type_by_name` + `Intl.*` fallback remains the canonical path.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 37 | 28 | 984 | 2 | 434 | 36 |
| monorepo-006 after | 20 | 20 | 976 | 1 | 434 | 20 |
| delta | -17 | -8 | -8 | -1 | 0 | -16 |

Diagnostics count is unchanged (`10,198` on both runs).

## Miss Classification

| Bucket | Baseline | After |
| --- | ---: | ---: |
| `target_declaration_files` | 28 | 20 |
| `target_source_files` | 0 | 0 |
| `by_kind.type_alias` | 16 | 17 |
| `by_kind.interface` | 12 | 3 |

The remaining declaration-file residue is now 20 misses: 17 type aliases plus
3 interfaces.

## Decision

1. Keep the expanded `Intl.*` fallback set and `Symbol`/`IteratorObject`
   admissions.
2. Keep alias symbols on the existing fallback path; alias residue is now the
   dominant bucket.
3. Next slice should target the 3 remaining interface misses (likely
   declaration-arena mismatch / value-merged edge cases) or pivot to the larger
   alias bucket with conformance-backed admission.
