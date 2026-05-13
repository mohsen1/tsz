# 2026-05-13 - DelegateCrossArenaSymbol Actual-Lib Allowlist Expansion

Attribution-mode follow-up for #6260. This run expands the conservative
actual-lib direct-interface allowlist and switches that direct path to
`resolve_lib_type_with_params`.

## Reproducer

| Item | Value |
| --- | --- |
| `tsz` code commit | local branch before PR update |
| baseline commit | `1b5c17a198` (`perf(checker): allowlist actual lib direct interfaces`) |
| `tsz` build | `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo build -p tsz-cli --bin tsz --features perf-tools --release` |
| Fixture generator | `scripts/bench/scale-cliff/generate-fixtures.sh --clean` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Timing feature | `tsz-common/perf-counters-timing` ON via `perf-tools` |
| Machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-13-allowlist-baseline-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-allowlist-baseline-monorepo-006-pc.json`
- After expansion:
  `docs/plan/perf-runs/raw/2026-05-13-allowlist-expanded-with-params-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-allowlist-expanded-with-params-monorepo-006-pc.json`

The synthetic fixture emits diagnostics, so `tsz` exits with code `2`. The JSON
artifacts are still written and are the source of truth below.

## Change

`direct_actual_lib_symbol_type` keeps the existing safety gates
(`SymbolArena` source, builtin-lib-only declarations, no value symbols, no type
aliases, single declaration), then:

1. expands the interface-name allowlist with measured residual pure interfaces
   (`IteratorYieldResult`, `IteratorReturnResult`, `Iterable`,
   `RegExpStringIterator`, etc.),
2. resolves allowlisted interfaces via `resolve_lib_type_with_params` instead
   of `resolve_lib_type_by_name`.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 55 | 40 | 991 | 3 | 434 | 54 |
| monorepo-006 after | 33 | 30 | 976 | 1 | 434 | 32 |
| delta | -22 | -10 | -15 | -2 | 0 | -22 |

## Miss Classification

| Bucket | Baseline | After |
| --- | ---: | ---: |
| `target_declaration_files` | 40 | 30 |
| `target_source_files` | 0 | 0 |
| `by_kind.type_alias` | 16 | 16 |
| `by_kind.interface` | 24 | 14 |

The unresolved residue is now 30 declaration-file misses: 16 type aliases plus
14 interfaces.

## Phase Split

Attribution-mode wall time is not comparable to timing-mode runs.

| Fixture | total s | check s | check % | diagnostics |
| --- | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 94.78 | 93.39 | 98.5 | 10,198 |
| monorepo-006 after | 102.92 | 101.02 | 98.2 | 10,198 |

## Decision

1. Keep the expanded interface allowlist and `resolve_lib_type_with_params`
   direct path.
2. Keep type aliases out of this slice; the remaining alias misses need a
   separate conformance-backed strategy.
