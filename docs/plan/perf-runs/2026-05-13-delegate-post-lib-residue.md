# 2026-05-13 - DelegateCrossArenaSymbol Post-Lib Residue

Attribution-mode refresh after #6314 and #6302 landed on `main`. This run
records the current declaration-file residue before the next code slice.

## Reproducer

| Item | Value |
| --- | --- |
| `tsz` code commit | `b9e0bf4e8d` |
| `tsz` build | `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=.target cargo build -p tsz-cli --bin tsz --features perf-tools --release` |
| Fixture generator | `scripts/bench/scale-cliff/generate-fixtures.sh` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Timing feature | `tsz-common/perf-counters-timing` ON via `perf-tools` |
| Machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- `docs/plan/perf-runs/raw/2026-05-13-delegate-post-lib-residue-monorepo-006-diag.json`
- `docs/plan/perf-runs/raw/2026-05-13-delegate-post-lib-residue-monorepo-006-pc.json`

The fixture still exits with code `2` because it intentionally emits
diagnostics. The JSON files are still written and are the artifacts used below.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 current main | 39 | 30 | 984 | 2 | 434 | 38 |

## Miss Classification

| Bucket | Count |
| --- | ---: |
| `target_declaration_files` | 30 |
| `target_source_files` | 0 |
| `by_kind.type_alias` | 16 |
| `by_kind.interface` | 14 |

Temporary local tracing at the fallback point produced this declaration-file
residue:

| Kind | Names |
| --- | --- |
| Type aliases | `DecoratorMetadata`, `DecoratorMetadataObject`, `FlatArray` x2, `IteratorResult` x2, `LocalesArgument`, `NumberFormatOptionsCurrencyDisplay`, `NumberFormatOptionsStyle`, `NumberFormatOptionsUseGrouping`, `Partial`, `PropertyKey`, `Readonly`, `Record` x2, `UnicodeBCP47LocaleIdentifier` |
| Interfaces | `ArrayIterator`, `DateTimeFormatOptions`, `Function`, `Iterator`, `Locale`, `NumberFormatOptions`, `NumberFormatOptionsCurrencyDisplayRegistry`, `NumberFormatOptionsStyleRegistry`, `NumberFormatOptionsUseGroupingRegistry`, `Object`, `RegExp`, `RegExpStringIterator`, `StringIterator`, `Symbol` |

## Phase Split

Attribution-mode wall time is not comparable to `tsgo` or timing-mode `tsz`.
Use these numbers only for phase dominance.

| Fixture | total s | check s | check % | diagnostics |
| --- | ---: | ---: | ---: | ---: |
| monorepo-006 | 90.60 | 89.06 | 98.3 | 10,198 |

## Decision

1. Treat the current declaration-file target as 30 misses: 16 utility aliases
   and 14 interfaces.
2. Keep type aliases on fallback until alias application/indexed-access
   behavior has a conformance-backed direct path.
3. The next interface code slice should focus on one tightly scoped merged-lib
   or namespace-qualified family, not a broad relaxation of value or multi-decl
   guards.
