# Delegate Lib Utility Aliases - 2026-05-13

This run admits a narrow actual-lib type-alias slice for the repeated
declaration-file residue rows identified by the previous attribution run:
`FlatArray`, `IteratorResult`, and `Record`.

The implementation is intentionally guarded:

- target arena must be an actual built-in lib declaration arena
- source must be `symbol_arenas`
- symbol must be type-only
- every declaration must match the exact alias name in an actual lib arena
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
| Files | 5,332 | 5,332 |
| Diagnostics | 10,198 | 10,198 |
| Total | 82.45s | 83.39s |
| Check | 80.76s | 81.82s |
| `delegate.calls` | 984 | 984 |
| `delegate.cache_hits_lib` | 2 | 2 |
| `delegate.cache_hits_cross_file` | 434 | 434 |
| `delegate.misses` | 38 | 35 |
| `checker.with_parent_cache_constructed` | 39 | 36 |
| `DelegateCrossArenaSymbol` children | 30 | 27 |

The three repeated utility aliases each drop from count 2 to count 1:

| Symbol | Before | After | Target file |
| --- | ---: | ---: | --- |
| `FlatArray` | 2 | 1 | `lib.es2019.array.d.ts` |
| `IteratorResult` | 2 | 1 | `lib.es2015.iterable.d.ts` |
| `Record` | 2 | 1 | `lib.es5.d.ts` |

The result is therefore a measured 3-child reduction, not the optimistic
6-child reduction implied by the aggregate repeated rows. The remaining
one-per-name rows likely come through a distinct symbol path or need a cleaner
lib alias body query that does not depend on the current broad type-reference
resolver.

## Next Target

Do not expand this allowlist mechanically. The bigger follow-up should extract
a dedicated direct lib type-alias body path, then route declaration-file aliases
through the typed cross-file query cache or prepopulate canonical lib alias
bodies in `DefinitionStore`. That would give reviewable coverage for all
actual-lib aliases while keeping unsupported declaration-file aliases on the
child-checker fallback.
