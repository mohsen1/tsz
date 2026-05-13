# perf(checker): route symbol-arena source-file delegations through cross-file cache

## Claim

`DelegateCrossArenaSymbol` misses from `binder.symbol_arenas` currently bypass
the canonical `CrossFileQueryKind::SymbolType` cache readers whenever the
symbol arena is discovered directly instead of through `resolve_symbol_file_index`.
The post-#5863 attribution run shows this is the dominant hot path: monorepo-006
constructs 924 `DelegateCrossArenaSymbol` child checkers, all from
`symbol_arenas`, with 883 targeting source files.

## Scope

- For `delegate_cross_arena_symbol_resolution`, detect non-current
  `symbol_arenas` targets that map to source files.
- Use the existing `cached_cross_file_symbol_type` /
  `cache_cross_file_symbol_type` helpers for those targets.
- Keep declaration-file / lib-style delegations, and programs with module
  augmentations, on the existing `lib_delegation_cache` / child-checker
  fallback path. Module augmentation can change source-file symbol answers
  based on importer graph state, so the shared `(file_idx, SymbolId)` key is
  intentionally disabled for those programs.
- Keep child-checker fallback and diagnostics behavior unchanged.

## Expected signal

- `cross_file_cache_miss_causes` becomes non-zero for the symbol-arena
  source-file path instead of staying all zero.
- Repeated source-file symbol-arena delegations can increment
  `delegate.cache_hits_cross_file`.
- `DelegateCrossArenaSymbol` construction count should drop on fixtures with
  repeated symbol-arena source-file lookups.

## Verification

- `cargo test -p tsz-checker --lib cross_file_query`
- `cargo test -p tsz-checker --lib`
- `cargo fmt --all -- --check`

## Local attribution check

`TSZ_PERF_COUNTERS=1 .target/release/tsz --extendedDiagnostics --noEmit -p
scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json
--perf-counters-json /tmp/tsz-6071-symbol-arena-monorepo-006-pc.json`
exits non-zero because the generated fixture emits expected diagnostics, but
it writes perf JSON.

The module-augmentation-guarded implementation observed on `monorepo-006`:

- `delegate.cache_hits_cross_file = 632` (previous refreshed run: `0`).
- `DelegateCrossArenaSymbol = 292` child checkers (previous refreshed run:
  `924`).
- `cross_file_cache_miss_causes`: `bucket_empty = 251`, other buckets `0`.
- Remaining `DelegateCrossArenaSymbol` misses: 251 source-file targets plus
  41 declaration-file targets.

The unguarded prototype regressed the
`moduleAugmentationImportsAndExports*` conformance group by caching
source-file symbol answers in a program with module augmentation. The PR now
detects module augmentations through the global augmentation indexes / binders
and leaves those programs on the old child-checker path. Local targeted
conformance: `./scripts/conformance/conformance.sh run --workers 4 --filter
moduleAugmentationImportsAndExports` passes 6/6.
