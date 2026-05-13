# Delegate Lib Utility Aliases - 2026-05-13

This run admits the actual-lib type-alias row from the repeated
declaration-file residue table whose alias body matches the supported
indexed type-literal plus conditional-index utility shape. On the current
fixture that is the row reported as `FlatArray`. The earlier exploratory branch
also tried `IteratorResult` and `Record`, but hosted conformance regressed
iterator-return fingerprints and recursive mapped type relationships, so those
alias body shapes stay on the fallback path.

The implementation is intentionally guarded:

- target arena must be an actual built-in lib declaration arena
- source must be `symbol_arenas`
- symbol must be type-only
- every declaration must match the exact alias name in an actual lib arena
- every alias declaration must have the supported indexed-access body shape
- alias lowering reuses the existing paired body/type-parameter resolver

## Reproducer

```bash
scripts/bench/scale-cliff/generate-fixtures.sh
CARGO_BUILD_JOBS=1 cargo build --release -p tsz-cli --features perf-tools
TSZ_TYPESCRIPT_LIB_DIR=/Users/mohsen/code/tsz/scripts/node_modules/typescript/lib \
TSZ_PERF_COUNTERS=1 \
.target/release/tsz --extendedDiagnostics --noEmit \
  -p scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json \
  --diagnostics-json docs/plan/perf-runs/raw/2026-05-13-delegate-lib-utility-aliases-monorepo-006-diag.json \
  --perf-counters-json docs/plan/perf-runs/raw/2026-05-13-delegate-lib-utility-aliases-monorepo-006-pc.json
```

The command exits with status 2 because the synthetic fixture intentionally
emits TypeScript diagnostics. It still writes both JSON artifacts.

## Result

| Metric | Before | After |
| --- | ---: | ---: |
| Files | 5,337 | 5,337 |
| Diagnostics | 10,198 | 10,198 |
| Total | 94.70s | 85.44s |
| Check | 92.48s | 83.39s |
| `delegate.calls` | 977 | 977 |
| `delegate.cache_hits_lib` | 1 | 1 |
| `delegate.cache_hits_cross_file` | 434 | 434 |
| `delegate.misses` | 30 | 29 |
| `checker.with_parent_cache_constructed` | 31 | 30 |
| `DelegateCrossArenaSymbol` children | 28 | 27 |

The indexed utility alias row drops from count 2 to count 1; `IteratorResult`
and `Record` remain on fallback after the conformance failure on the broader
exploratory branch:

| Symbol | Before | After | Target file |
| --- | ---: | ---: | --- |
| `FlatArray` | 2 | 1 | `lib.es2019.array.d.ts` |
| `IteratorResult` | 2 | 2 | `lib.es2015.iterable.d.ts` |
| `Record` | 2 | 2 | `lib.es5.d.ts` |

The result is therefore a measured 1-child reduction. The remaining
two-per-name rows need a cleaner lib alias body query that preserves the
declared alias shape for recursive and iterator-sensitive diagnostics.

## Next Target

Do not expand this mechanically. The broader exploratory branch regressed
conformance below the aggregate tolerance. The bigger follow-up should extract
a dedicated direct lib type-alias body path, then route declaration-file aliases
through the typed cross-file query cache or prepopulate canonical lib alias
bodies in `DefinitionStore`. That would give reviewable coverage for all
actual-lib aliases while keeping unsupported declaration-file aliases on the
child-checker fallback.
