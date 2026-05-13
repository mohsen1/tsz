# 2026-05-13 - DelegateCrossArenaSymbol Source-File Direct Interface

Attribution-mode follow-up for #6212. This run validates a direct typed query
for the source-file interface subset of the remaining stable source-file
symbol-arena cold reads.

## Reproducer

| Item | Value |
| --- | --- |
| `tsz` code commit | local branch before PR publication |
| `origin/main` base | `1607ca4c04` |
| `tsz` build | `CARGO_TARGET_DIR=.target cargo build -p tsz-cli --bin tsz --features perf-tools --release` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Timing feature | `tsz-common/perf-counters-timing` ON via `perf-tools` |
| Machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- `docs/plan/perf-runs/raw/2026-05-13-delegate-source-file-direct-interface-monorepo-006-diag.json`
- `docs/plan/perf-runs/raw/2026-05-13-delegate-source-file-direct-interface-monorepo-006-pc.json`

The synthetic fixture still emits diagnostics, so `tsz` exits non-zero. The
diagnostics and perf-counter JSON files are still written and are the artifacts
used below.

## Change

The existing direct interface lowering path now has a source-file mode, but
only when the stable source-file symbol-arena cache proof has already passed.
The source-file mode accepts only scope-independent interface declarations:

- no interface type parameters,
- no heritage clauses,
- no computed member names,
- property signatures only,
- member annotations made from primitive keywords, literal types,
  union/intersection composition, arrays/tuples, or wrapped forms of those.

Interfaces whose member annotations reference target-file names still use the
existing child-checker path.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 before (#6212) | 554 | 539 | 941 | 3 | 385 | 553 |
| monorepo-006 after | 307 | 292 | 941 | 3 | 385 | 306 |
| delta | -247 | -247 | 0 | 0 | 0 | -247 |

The source-file direct interface query removes 247 child-checker constructions
without changing delegate call volume or cross-file cache hits.

## Direct Lowering Signal

| Outcome | Before (#6212) | After |
| --- | ---: | ---: |
| `success` | 0 | 247 |
| `rejected_non_direct_arena` | 539 | 41 |
| `not_interface` | 0 | 251 |
| `complex_declaration` | 0 | 0 |

`cross_file_cache_miss_causes.bucket_empty` remains 498 because the cache probe
still happens before the direct query. After this PR, 247 of those cold probes
are handled by direct lowering instead of constructing a child checker.

## Remaining Residue

The remaining 292 `DelegateCrossArenaSymbol` child-checker constructions split
as:

| Slice | Count | Source |
| --- | ---: | --- |
| Source-file variables | 251 | `delegate_miss_classification.by_kind.variable`; `target_source_files = 251`. |
| Source-file interfaces not handled by this direct query | 25 | `delegate_miss_classification.by_kind.interface`. |
| Declaration-file targets / type aliases | 41 | `target_declaration_files = 41`; `by_kind.type_alias = 16` contributes here. |

## Phase Split

Attribution-mode wall time is not comparable to `tsgo` or timing-mode `tsz`.
Use these numbers only for phase dominance.

| Fixture | total s | check s | check % | diagnostics |
| --- | ---: | ---: | ---: | ---: |
| monorepo-006 | 75.37 | 73.49 | 97.5 | 10,198 |

## Decision

1. Keep the source-file direct interface query. It removes 247
   `DelegateCrossArenaSymbol` child-checker constructions on monorepo-006 while
   preserving the stable source-file cache proof as the source-file entry gate.
2. Do not broaden source-file interface direct lowering to scope-dependent
   member annotations without a target-file name-resolution proof.
3. The next `DelegateCrossArenaSymbol` target is the 251 source-file variable
   child-checker constructions or the 41 declaration-file target slice.

