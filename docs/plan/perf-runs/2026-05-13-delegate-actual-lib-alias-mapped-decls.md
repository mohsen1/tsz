# 2026-05-13 - DelegateCrossArenaSymbol Actual-Lib Alias Mapped-Decl Follow-up

Attribution-mode follow-up on top of #6440 (`Intl Locales/Unicode alias`).
This slice removes the remaining declaration-file alias residue by admitting
allowlisted actual-lib aliases when the merged symbol has unmapped declaration
indices but every mapped declaration is still a builtin-lib declaration.

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `7ec1b5d136` (`docs(perf): record Intl locales/unicode alias attribution`) |
| `tsz` build (after) | `cargo build -p tsz-cli --release --features perf-tools` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-alias-mapped-decls-baseline-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-alias-mapped-decls-baseline-monorepo-006-pc.json`
- After:
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-alias-mapped-decls-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-alias-mapped-decls-after-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

## Change

In `direct_actual_lib_symbol_type`:

1. extend the narrow alias allowlist with:
   - `Record`
   - `Readonly`
   - `Partial`
   - `IteratorResult`
   - `FlatArray`
   - `PropertyKey`
   - `DecoratorMetadataObject`
   - `DecoratorMetadata`
2. for allowlisted aliases, try direct alias-by-symbol lowering first on the
   current symbol (while preserving existing `Intl.*` namespace-symbol fallback),
3. mirror the existing `Iterator` unmapped-declaration gate for allowlisted
   aliases (`mapped builtin-lib declarations` and `has_unmapped` acceptance).

Added focused unit coverage:

- `direct_actual_lib_symbol_type_handles_record_alias_symbol`
- `direct_actual_lib_symbol_type_handles_iterator_result_alias_symbol`
- `direct_actual_lib_symbol_type_handles_flat_array_alias_symbol`
- `direct_actual_lib_symbol_type_handles_readonly_alias_symbol`
- `direct_actual_lib_symbol_type_handles_property_key_alias_symbol`
- `direct_actual_lib_symbol_type_handles_partial_alias_symbol`
- `direct_actual_lib_symbol_type_handles_decorator_metadata_object_alias_symbol`
- `direct_actual_lib_symbol_type_handles_decorator_metadata_alias_symbol`

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 11 | 11 | 975 | 0 | 434 | 11 |
| monorepo-006 after | 0 | 0 | 997 | 26 | 434 | 0 |
| delta | -11 | -11 | +22 | +26 | 0 | -11 |

Diagnostics count is unchanged (`10,198` on both runs).

## Miss Classification

| Bucket | Baseline | After |
| --- | ---: | ---: |
| `target_declaration_files` | 11 | 0 |
| `target_source_files` | 0 | 0 |
| `by_kind.type_alias` | 11 | 0 |
| `by_kind.interface` | 0 | 0 |

The declaration-file residue is now `0` for monorepo-006.

## Decision

1. Keep the allowlisted mapped-declaration alias gate and direct alias-by-symbol
   lowering path.
2. Keep all non-allowlisted aliases on existing fallback paths.
3. With `DelegateCrossArenaSymbol = 0` on monorepo-006, next Tier 2 work should
   move to the next measured reason on the cliff rather than further alias
   expansions.
