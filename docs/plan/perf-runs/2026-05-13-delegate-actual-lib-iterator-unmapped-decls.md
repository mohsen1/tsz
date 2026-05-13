# 2026-05-13 - DelegateCrossArenaSymbol Actual-Lib Iterator Unmapped-Decl Follow-up

Attribution-mode follow-up on top of #6398 (`Intl.Locale heritage`).
This slice targets the final declaration-file interface miss by admitting the
`Iterator` symbol shape that carries unmapped declaration indices in the merged
binder map.

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `2c6920a6a3` (`docs(perf): record Intl.Locale heritage attribution`) |
| `tsz` build (after) | `cargo build -p tsz-cli --release --features perf-tools` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-iterator-unmapped-decls-baseline-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-iterator-unmapped-decls-baseline-monorepo-006-pc.json`
- After:
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-iterator-unmapped-decls-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-iterator-unmapped-decls-after-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

## Change

In `direct_actual_lib_symbol_type`:

1. add `Iterator` to the targeted `resolve_lib_type_with_params` probe set,
2. keep the existing strict direct actual-lib declaration gate as default,
3. for `Iterator` only, allow the direct path when mapped declarations are
   builtin-lib-only and declaration names match, or when the merged symbol also
   carries unmapped declaration indices (observed in this residual path).

This preserves strict behavior for all other symbols while covering the final
known declaration-file interface miss.

Added focused unit coverage:

- `direct_actual_lib_symbol_type_handles_builtin_iterator_symbol`

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` | delegate calls | lib hits | cross-file hits | misses |
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
| `by_kind.type_alias` | 17 | 17 |
| `by_kind.interface` | 1 | 0 |

The remaining declaration-file residue is now 17 misses, all type aliases.

## Decision

1. Keep the targeted `Iterator` admission that tolerates unmapped declaration
   indices only for this builtin-lib symbol.
2. Keep all alias symbols on existing fallback paths.
3. Next slices should pivot from interface admissions to alias-focused,
   conformance-backed proof work.
