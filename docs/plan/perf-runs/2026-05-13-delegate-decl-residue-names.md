# Delegate Declaration-File Residue Names - 2026-05-13

This run adds bounded symbol-level attribution for the declaration-file
`DelegateCrossArenaSymbol` residue left after #6260/#6286/#6314.

## Reproducer

```bash
scripts/bench/scale-cliff/generate-fixtures.sh
CARGO_BUILD_JOBS=1 cargo build --release -p tsz-cli --features perf-tools
TSZ_TYPESCRIPT_LIB_DIR=/Users/mohsen/code/tsz/scripts/node_modules/typescript/lib \
TSZ_PERF_COUNTERS=1 \
.target/release/tsz --extendedDiagnostics --noEmit \
  -p scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json \
  --diagnostics-json docs/plan/perf-runs/raw/2026-05-13-delegate-decl-residue-names-monorepo-006-diag.json \
  --perf-counters-json docs/plan/perf-runs/raw/2026-05-13-delegate-decl-residue-names-monorepo-006-pc.json
```

The command exits with status 2 because the synthetic fixture intentionally
emits TypeScript diagnostics. It still writes both JSON artifacts.

## Result

| Metric | Value |
| --- | ---: |
| Files | 5,332 |
| Diagnostics | 10,198 |
| Total | 82.45s |
| Check | 80.76s |
| `delegate.calls` | 984 |
| `delegate.cache_hits_lib` | 2 |
| `delegate.cache_hits_cross_file` | 434 |
| `delegate.misses` | 38 |
| `checker.with_parent_cache_constructed` | 39 |
| `DelegateCrossArenaSymbol` children | 30 |

The new `delegate_declaration_file_miss_residues` field reports 27 distinct
symbol/file rows accounting for the 30 declaration-file child-checker
constructions. All rows are `symbol_arenas` sourced.

| Symbol | Kind | Count | Target file |
| --- | --- | ---: | --- |
| `FlatArray` | type_alias | 2 | `lib.es2019.array.d.ts` |
| `IteratorResult` | type_alias | 2 | `lib.es2015.iterable.d.ts` |
| `Record` | type_alias | 2 | `lib.es5.d.ts` |
| `ArrayIterator` | interface | 1 | `lib.es2015.iterable.d.ts` |
| `DateTimeFormatOptions` | interface | 1 | `lib.es5.d.ts` |
| `DecoratorMetadata` | type_alias | 1 | `lib.decorators.d.ts` |
| `DecoratorMetadataObject` | type_alias | 1 | `lib.decorators.d.ts` |
| `Function` | interface | 1 | `lib.es5.d.ts` |
| `Iterator` | interface | 1 | `lib.esnext.iterator.d.ts` |
| `Locale` | interface | 1 | `lib.es2020.intl.d.ts` |
| `LocalesArgument` | type_alias | 1 | `lib.es2020.intl.d.ts` |
| `NumberFormatOptions` | interface | 1 | `lib.es5.d.ts` |
| `NumberFormatOptionsCurrencyDisplay` | type_alias | 1 | `lib.es5.d.ts` |
| `NumberFormatOptionsCurrencyDisplayRegistry` | interface | 1 | `lib.es5.d.ts` |
| `NumberFormatOptionsStyle` | type_alias | 1 | `lib.es5.d.ts` |
| `NumberFormatOptionsStyleRegistry` | interface | 1 | `lib.es5.d.ts` |
| `NumberFormatOptionsUseGrouping` | type_alias | 1 | `lib.es5.d.ts` |
| `NumberFormatOptionsUseGroupingRegistry` | interface | 1 | `lib.es5.d.ts` |
| `Object` | interface | 1 | `lib.es5.d.ts` |
| `Partial` | type_alias | 1 | `lib.es5.d.ts` |
| `PropertyKey` | type_alias | 1 | `lib.es5.d.ts` |
| `Readonly` | type_alias | 1 | `lib.es5.d.ts` |
| `RegExp` | interface | 1 | `lib.es5.d.ts` |
| `RegExpStringIterator` | interface | 1 | `lib.es2020.symbol.wellknown.d.ts` |
| `StringIterator` | interface | 1 | `lib.es2015.iterable.d.ts` |
| `Symbol` | interface | 1 | `lib.es5.d.ts` |
| `UnicodeBCP47LocaleIdentifier` | type_alias | 1 | `lib.es2020.intl.d.ts` |

## Next Target

The next implementation PR can stop treating the declaration-file tail as an
aggregate. The highest-leverage proof target is the repeated utility aliases
(`FlatArray`, `IteratorResult`, `Record`) because they account for 6 of the 31
remaining child-checker constructions measured before #6302, and 6 of the 30
remaining child-checker constructions on current main. Interfaces remain split
between plain `lib.es5.d.ts` globals, iterator helpers, and `Intl`/decorator
surfaces.
