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
  `symbol_arenas` class/interface targets that map to source files and
  whose single declaration is proven to live only in that source-file arena.
- Use the `SymbolType` cache bucket for those targets. Source-file
  symbol-arena entries add a program-local scope across the secondary key
  fields so virtual conformance programs that reuse small `file_idx` /
  `SymbolId` values cannot collide in the same process.
- Keep declaration-file / lib-style delegations, and programs with module
  augmentations, on the existing `lib_delegation_cache` / child-checker
  fallback path. Module augmentation can change source-file symbol answers
  based on importer graph state, so the shared `(file_idx, SymbolId)` key is
  intentionally disabled for those programs.
- Do not write generic payloads from this `symbol_arenas` path into the
  shared bucket.
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

The module-augmentation-guarded and program-scoped implementation observed on
`monorepo-006`:

- `delegate.cache_hits_cross_file = 96` (previous refreshed run: `0`).
- `DelegateCrossArenaSymbol = 828` child checkers (previous refreshed run:
  `924`).
- `cross_file_cache_miss_causes`: `bucket_empty = 247`, other buckets `0`.

The unguarded prototype regressed the
`moduleAugmentationImportsAndExports*` conformance group by caching
source-file symbol answers in a program with module augmentation. The PR now
detects module augmentations through the global augmentation indexes / binders.
The PR also scopes source-file symbol-arena cache keys by a process-local
program salt so unrelated virtual programs do not share `(file_idx, SymbolId)`
answers, requires a single class/interface declaration registered solely in the
delegated arena, and skips writes with type parameters before using the shared
bucket. Local targeted conformance:
`./scripts/conformance/conformance.sh run --workers 4 --filter
moduleAugmentationImportsAndExports` passes 6/6.
