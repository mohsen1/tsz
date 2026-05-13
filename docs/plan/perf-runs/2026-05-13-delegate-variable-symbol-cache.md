# 2026-05-13 - DelegateCrossArenaSymbol Variable Symbol Cache

Attribution-mode follow-up for #6203. This run validates the first
requester-independent variable-symbol slice for the stable source-file
symbol-arena cache key.

## Reproducer

| Item | Value |
| --- | --- |
| `tsz` code commit | `2792c8607a` before docs-only updates |
| `origin/main` base | `c4f68c6c90` |
| `tsz` build | `CARGO_TARGET_DIR=.target cargo build -p tsz-cli --bin tsz --features perf-tools --release` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Timing feature | `tsz-common/perf-counters-timing` ON via `perf-tools` |
| Machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- `docs/plan/perf-runs/raw/2026-05-13-delegate-variable-symbol-cache-monorepo-006-diag.json`
- `docs/plan/perf-runs/raw/2026-05-13-delegate-variable-symbol-cache-monorepo-006-pc.json`

The synthetic fixture still emits diagnostics, so `tsz` exits with code `2`.
The diagnostics and perf-counter JSON files are still written and are the
artifacts used below.

## Change

The stable source-file symbol-arena cache key now admits a conservative
variable-symbol subset:

- exactly one symbol declaration,
- the declaration belongs only to the delegated source-file arena,
- the declaration is a `VariableDeclaration` in that arena,
- the variable has an explicit TypeScript type annotation,
- merged module and alias symbols are rejected.

Inferred variables remain outside this proof.

This branch rebased over #6208, which renamed the coarse
`source_file_symbol_arena_cache_eligibility` array to the detailed
`source_file_symbol_arena_cache_eligibility_outcomes` array. The old
`eligible` bucket is now `cacheable`, `declaration_file` is now
`target_declaration_file`, and the old `unstable_symbol` residue appears as
the more specific rejection outcome that blocked cacheability.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 before (#6203) | 843 | 828 | 941 | 3 | 96 | 842 |
| monorepo-006 after | 554 | 539 | 941 | 3 | 385 | 553 |
| delta | -289 | -289 | 0 | 0 | +289 | -289 |

The variable slice converts 289 child-checker constructions into stable
source-file symbol-arena cache hits on this fixture.

## Eligibility

| Bucket | Before (#6203) | After | Interpretation |
| --- | ---: | ---: | --- |
| `cacheable` / old `eligible` | 343 | 883 | Classes/interfaces plus annotated variables can now form a stable key. |
| old `unstable_symbol` | 540 | 0 | The measured variable residue no longer fails the stability gate. In #6208 terms, `not_class_or_interface = 0` after this PR. |
| `target_declaration_file` / old `declaration_file` | 44 | 44 | Declaration-file targets are unchanged. |

All other eligibility buckets stay zero on this run.

The `eligible` count includes both cache hits and cold first reads. After this
PR, `cross_file_cache_miss_causes.bucket_empty = 498`, matching the remaining
source-file child-checker misses. All other detailed eligibility outcomes are
zero after the change.

## Remaining Residue

The remaining 539 `DelegateCrossArenaSymbol` child-checker constructions split
as:

| Slice | Count | Source |
| --- | ---: | --- |
| Stable source-file key, cold cache | 498 | `cross_file_cache_miss_causes.bucket_empty`; also `target_source_files = 498`. |
| Declaration-file targets | 41 | `delegate_miss_classification.target_declaration_files`; `declaration_file = 44` still includes 3 lib cache hits. |

The remaining source-file misses are first-requester cold reads, not stability
rejections. The next `DelegateCrossArenaSymbol` lever needs either a direct
typed query for the cold source-file slice or declaration-file target handling;
relaxing this variable proof further will not move monorepo-006.

## Phase Split

Attribution-mode wall time is not comparable to `tsgo` or timing-mode `tsz`.
Use these numbers only for phase dominance.

| Fixture | total s | check s | check % | diagnostics |
| --- | ---: | ---: | ---: | ---: |
| monorepo-006 | 82.98 | 80.90 | 97.5 | 10,198 |

## Decision

1. Keep the annotated-variable proof. It removes the whole measured
   `unstable_symbol` bucket on monorepo-006 while preserving the stable
   source-file program-scope key.
2. Do not broaden the variable slice in the next PR; there is no remaining
   variable stability residue on this fixture.
3. Target the 498 stable source-file cold reads next, or the 41 declaration-file
   symbol-arena misses if a smaller direct query emerges.
