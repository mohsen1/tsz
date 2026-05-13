# 2026-05-13 - DelegateCrossArenaSymbol Actual-Lib Intl SignDisplay Registry Follow-up

Attribution-mode follow-up on top of #6369 (`Intl fallback expansion`).
This slice targets one remaining declaration-file interface miss by admitting
`NumberFormatOptionsSignDisplayRegistry` into the direct actual-lib path.

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `23f7f83faa` (`docs(perf): record Intl fallback expansion attribution`) |
| `tsz` build (after) | `cargo build -p tsz-cli --release --features perf-tools` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-intl-sign-display-registry-baseline-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-intl-sign-display-registry-baseline-monorepo-006-pc.json`
- After:
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-intl-sign-display-registry-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-intl-sign-display-registry-after-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

## Change

`direct_actual_lib_symbol_type` now:

1. adds `NumberFormatOptionsSignDisplayRegistry` to the targeted `Intl.*`
   fallback allowlist,
2. adds `NumberFormatOptionsSignDisplayRegistry` to the targeted
   `resolve_lib_type_with_params` probe set,
3. when `Intl` namespace export resolution returns a different symbol id,
   resolves through that symbol only if its declarations pass the same
   direct actual-lib declaration gate.

Added focused unit coverage:

- `resolves_intl_namespace_exported_sign_display_registry_directly`
- `direct_actual_lib_symbol_type_handles_sign_display_registry_symbol`

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 20 | 20 | 976 | 1 | 434 | 20 |
| monorepo-006 after | 19 | 19 | 976 | 1 | 434 | 19 |
| delta | -1 | -1 | 0 | 0 | 0 | -1 |

Diagnostics count is unchanged (`10,198` on both runs).

## Miss Classification

| Bucket | Baseline | After |
| --- | ---: | ---: |
| `target_declaration_files` | 20 | 19 |
| `target_source_files` | 0 | 0 |
| `by_kind.type_alias` | 17 | 17 |
| `by_kind.interface` | 3 | 2 |

The remaining declaration-file residue is now 19 misses: 17 type aliases plus
2 interfaces.

## Decision

1. Keep the SignDisplay registry direct-path admission and namespace-symbol
   fallback gate.
2. Keep alias symbols on the existing fallback path.
3. Next slice should target the two remaining interface misses (`Iterator`
   declaration-gating shape and `Locale` heritage-lowering gap).
