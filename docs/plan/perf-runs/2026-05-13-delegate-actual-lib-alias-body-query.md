# 2026-05-13 - DelegateCrossArenaSymbol Actual-Lib Alias Body Query

Attribution-mode follow-up on current `origin/main`
(`d9f7f5d11c`, after #6495). This slice starts the actual-lib alias typed-query
path without reopening the generic utility-alias shortcut that failed full
conformance earlier.

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `d9f7f5d11c` |
| after branch | `perf/actual-lib-alias-body-query-20260513` |
| `tsz` build | `cargo build -p tsz-cli --release --features perf-tools` |
| fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| counter mode | `TSZ_PERF_COUNTERS=1` |
| machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-alias-body-query-baseline-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-alias-body-query-baseline-monorepo-006-pc.json`
- After:
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-alias-body-query-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-alias-body-query-after-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

## Change

`direct_actual_lib_symbol_type` now has a typed alias-body helper for a narrow
decorator-metadata actual-lib type-alias slice. The helper returns the
registered alias body only when:

1. the alias name is `DecoratorMetadata` or `DecoratorMetadataObject`,
2. every alias declaration is proven to come from an actual bundled-lib arena,
3. the existing lib resolver returns a `Lazy(DefId)`,
4. the `DefinitionStore` entry is a `TypeAlias`,
5. the `DefinitionStore` has a registered alias body, and
6. the alias has no type parameters.

Generic aliases such as `Record`, `Readonly`, `Partial`, `FlatArray`, and
`IteratorResult` still return `None` and use the existing child-checker
fallback. `PropertyKey` also stays on fallback after the broader non-generic
alias attempt failed conformance in assignability-sensitive lib signatures.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` children | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 31 | 28 | 977 | 1 | 434 | 30 |
| monorepo-006 after | 29 | 26 | 977 | 1 | 434 | 28 |
| delta | -2 | -2 | 0 | 0 | 0 | -2 |

Diagnostics count is unchanged (`10,198` on both runs).

## Miss Classification

| Bucket | Baseline | After |
| --- | ---: | ---: |
| `target_declaration_files` | 28 | 26 |
| `target_source_files` | 0 | 0 |
| `by_kind.type_alias` | 16 | 14 |
| `by_kind.interface` | 12 | 12 |

Reduced declaration-file alias residue:

- `DecoratorMetadata`: `1 -> 0`
- `DecoratorMetadataObject`: `1 -> 0`

## Decision

Keep this as the first behavior-changing typed alias-body query slice. It
removes the decorator metadata residue while keeping generic utility aliases
and `PropertyKey` on fallback. The direct path caches the registered alias body,
not the opaque `Lazy(DefId)` wrapper, because the wrapper is not a sound
symbol-type substitute in assignability and constraint checks. The next alias
PR should avoid expanding this name list and instead build a canonical,
generic-aware alias query/application path that preserves alias shape and passes
the utility-alias conformance regressions before admission.
