# 2026-05-13 - DelegateCrossArenaSymbol Actual-Lib Intl Locale Heritage Follow-up

Attribution-mode follow-up on top of #6395 (`Intl sign-display registry`).
This slice targets one remaining declaration-file interface miss by admitting
`Intl.Locale` to direct interface lowering even though it has heritage clauses.

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `535bdb009d` (`docs(perf): record Intl sign-display registry attribution`) |
| `tsz` build (after) | `cargo build -p tsz-cli --release --features perf-tools` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-intl-locale-heritage-baseline-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-intl-locale-heritage-baseline-monorepo-006-pc.json`
- After:
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-intl-locale-heritage-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-intl-locale-heritage-after-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

## Change

`resolve_lib_interface_type_by_symbol` currently rejects any interface symbol
that has heritage clauses. For this slice, keep the guard for all other
interfaces but allow direct lowering for `cache_name == "Intl.Locale"`.

This preserves the existing conservative policy while admitting one known,
stable residual interface miss from the actual-lib direct path.

Added focused unit coverage:

- `resolves_intl_namespace_exported_locale_directly`

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 19 | 19 | 976 | 1 | 434 | 19 |
| monorepo-006 after | 18 | 18 | 976 | 1 | 434 | 18 |
| delta | -1 | -1 | 0 | 0 | 0 | -1 |

Diagnostics count is unchanged (`10,198` on both runs).

## Miss Classification

| Bucket | Baseline | After |
| --- | ---: | ---: |
| `target_declaration_files` | 19 | 18 |
| `target_source_files` | 0 | 0 |
| `by_kind.type_alias` | 17 | 17 |
| `by_kind.interface` | 2 | 1 |

The remaining declaration-file residue is now 18 misses: 17 type aliases plus
1 interface.

## Decision

1. Keep the narrow `Intl.Locale` heritage admission in the namespace-direct
   lowering helper.
2. Keep aliases on existing fallback paths.
3. Next slice should target the final interface miss (`Iterator`
   declaration-shape gating) or pivot to alias-focused proof slices.
