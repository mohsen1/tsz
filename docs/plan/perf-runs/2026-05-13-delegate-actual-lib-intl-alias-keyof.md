# 2026-05-13 - DelegateCrossArenaSymbol Actual-Lib Intl Alias Keyof Follow-up

Attribution-mode follow-up on top of #6417 (`Iterator` unmapped-decl).
This slice targets part of the remaining declaration-file type-alias residue by
admitting a narrow set of non-generic `Intl` option aliases in the direct
actual-lib path.

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `a5eb236f6d` (`docs(perf): record Iterator unmapped-decl attribution`) |
| `tsz` build (after) | `cargo build -p tsz-cli --release --features perf-tools` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-intl-alias-keyof-baseline-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-intl-alias-keyof-baseline-monorepo-006-pc.json`
- After:
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-intl-alias-keyof-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-intl-alias-keyof-after-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

## Change

In `direct_actual_lib_symbol_type`:

1. admit a narrow actual-lib alias allowlist:
   - `NumberFormatOptionsCurrencyDisplay`
   - `NumberFormatOptionsSignDisplay`
   - `NumberFormatOptionsStyle`
   - `NumberFormatOptionsUseGrouping`
2. for this alias allowlist, resolve via `Intl` namespace symbol using a new
   `resolve_lib_alias_type_by_symbol` helper,
3. keep strict default behavior for all other type aliases.

Added focused unit coverage:

- `direct_actual_lib_symbol_type_handles_intl_sign_display_alias_symbol`

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 17 | 17 | 976 | 1 | 434 | 17 |
| monorepo-006 after | 13 | 13 | 976 | 1 | 434 | 13 |
| delta | -4 | -4 | 0 | 0 | 0 | -4 |

Diagnostics count is unchanged (`10,198` on both runs).

## Miss Classification

| Bucket | Baseline | After |
| --- | ---: | ---: |
| `target_declaration_files` | 17 | 13 |
| `target_source_files` | 0 | 0 |
| `by_kind.type_alias` | 17 | 13 |
| `by_kind.interface` | 0 | 0 |

The remaining declaration-file residue is now 13 misses, all type aliases.

## Decision

1. Keep the narrow `Intl` alias allowlist + namespace-alias resolver path.
2. Keep all non-allowlisted aliases on existing fallback paths.
3. Next slices should continue alias-focused, conformance-backed admissions.
