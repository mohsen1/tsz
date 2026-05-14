# 2026-05-14 - DelegateCrossArenaSymbol Missing Resolver Alias Follow-up

Attribution-mode follow-up on top of current `main` (`8fdd425435`) targeting
remaining actual-lib declaration-file alias misses where resolver-backed alias
lookup returned `None`.

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `8fdd425435` |
| after branch | `codex/perf-alias-missing-resolver-main-20260514` |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| fixture path | `/private/tmp/tsz-bench-fixtures/monorepo-006` |
| counter mode | `TSZ_PERF_COUNTERS=1` |
| machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-missing-resolver-baseline-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-missing-resolver-baseline-monorepo-006-pc.json`
- After:
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-missing-resolver-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-missing-resolver-after-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

## Change

- Extend the direct actual-lib alias allowlist with:
  - `LocalesArgument`
  - `NumberFormatOptionsCurrencyDisplay`
  - `NumberFormatOptionsSignDisplay`
  - `NumberFormatOptionsStyle`
  - `NumberFormatOptionsUseGrouping`
  - `UnicodeBCP47LocaleIdentifier`
- When resolver-backed actual-lib alias lookup misses, lower the alias body
  directly from proven declaration arenas into `DefinitionStore` and continue
  the existing direct delegation path instead of immediately returning
  `MissingResolverType`.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` children | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 13 | 13 | 977 | 0 | 434 | 13 |
| monorepo-006 after | 5 | 5 | 975 | 0 | 434 | 5 |
| delta | -8 | -8 | -2 | 0 | 0 | -8 |

Diagnostics count is unchanged (`10,198` on both runs).

## Miss Classification

| Bucket | Baseline | After |
| --- | ---: | ---: |
| `target_declaration_files` | 13 | 5 |
| `target_source_files` | 0 | 0 |
| `by_kind.type_alias` | 11 | 5 |
| `by_kind.interface` | 2 | 0 |

Declaration-file residue rows removed:

- `LocalesArgument` (count `1`)
- `NumberFormatOptionsCurrencyDisplay` (count `1`)
- `NumberFormatOptionsSignDisplay` (count `1`)
- `NumberFormatOptionsStyle` (count `1`)
- `NumberFormatOptionsUseGrouping` (count `1`)
- `TextInfo` (count `1`)
- `UnicodeBCP47LocaleIdentifier` (count `1`)
- `WeekInfo` (count `1`)

Remaining declaration-file residue:

- `FlatArray` (count `2`)
- `IteratorResult` (count `2`)
- `Partial` (count `1`)

Alias-body outcomes shift:

- `success`: `6 -> 12`
- `missing_resolver_type`: `6 -> 0`
- `generic_alias`: unchanged at `5`

## Decision

Keep this resolver-miss alias follow-up. It removes eight declaration-file
`DelegateCrossArenaSymbol` misses on monorepo-006 with unchanged diagnostics and
leaves the remaining generic utility aliases (`FlatArray`, `IteratorResult`,
`Partial`) on fallback.
