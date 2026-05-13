# 2026-05-13 - DelegateCrossArenaSymbol Actual-Lib Intl Locales/Unicode Alias Follow-up

Attribution-mode follow-up on top of #6430 (`Intl alias-keyof`).
This slice targets two remaining declaration-file type-alias misses by
admitting `Intl.LocalesArgument` and
`Intl.UnicodeBCP47LocaleIdentifier` through the same namespace-symbol alias
lowering path.

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `11e8d247a4` (`docs(perf): record Intl alias-keyof attribution`) |
| `tsz` build (after) | `cargo build -p tsz-cli --release --features perf-tools` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-intl-alias-locales-unicode-baseline-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-intl-alias-locales-unicode-baseline-monorepo-006-pc.json`
- After:
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-intl-alias-locales-unicode-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-intl-alias-locales-unicode-after-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

## Change

In `direct_actual_lib_symbol_type`:

1. add `LocalesArgument` and `UnicodeBCP47LocaleIdentifier` to the targeted
   direct actual-lib alias allowlist,
2. reuse the existing `Intl` namespace-symbol alias resolver path for these
   aliases,
3. keep strict fallback behavior for all non-allowlisted aliases.

Added focused unit coverage:

- `direct_actual_lib_symbol_type_handles_intl_locales_argument_alias_symbol`
- `direct_actual_lib_symbol_type_handles_intl_unicode_bcp47_alias_symbol`

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 13 | 13 | 975 | 0 | 434 | 13 |
| monorepo-006 after | 11 | 11 | 975 | 0 | 434 | 11 |
| delta | -2 | -2 | 0 | 0 | 0 | -2 |

Diagnostics count is unchanged (`10,198` on both runs).

## Miss Classification

| Bucket | Baseline | After |
| --- | ---: | ---: |
| `target_declaration_files` | 13 | 11 |
| `target_source_files` | 0 | 0 |
| `by_kind.type_alias` | 13 | 11 |
| `by_kind.interface` | 0 | 0 |

The remaining declaration-file residue is now 11 misses, all type aliases.

## Decision

1. Keep the narrow `Intl` alias allowlist expansion for
   `LocalesArgument` and `UnicodeBCP47LocaleIdentifier`.
2. Keep non-allowlisted aliases on existing fallback paths.
3. Next slices should continue targeted alias admissions with conformance
   verification.
