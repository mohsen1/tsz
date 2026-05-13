# 2026-05-13 - DelegateCrossArenaSymbol Actual-Lib Core Value-Interface Allowlist

Attribution-mode follow-up on top of the merged-declaration actual-lib direct
path. This run admits a narrowly scoped value-bearing interface set through the
same direct gateway.

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `e38a96cf19` (`chore: rebase actual-lib allowlist branch on main`) |
| after commit | `1fa5be3775` (`perf(checker): admit selected value-bearing actual-lib interfaces`) |
| baseline build | `CARGO_TARGET_DIR=/tmp/tsz-target-corevalue-rebase-base cargo build -p tsz-cli --bin tsz --features perf-tools --release` |
| after build | `CARGO_TARGET_DIR=/tmp/tsz-target-corevalue-rebase-after cargo build -p tsz-cli --bin tsz --features perf-tools --release` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-core-value-baseline-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-core-value-baseline-monorepo-006-pc.json`
- After:
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-core-value-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-actual-lib-core-value-after-monorepo-006-pc.json`

The synthetic fixture still emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

## Change

`direct_actual_lib_symbol_type` keeps the existing direct-path safety gates
(`SymbolArena` source, actual bundled-lib declaration arenas only, TYPE-only
symbol requirement, alias fallback path retained), but now admits a tiny
value-bearing interface allowlist:

- `Function`
- `Iterator`
- `Locale`
- `Object`
- `RegExp`

The resolver policy is unchanged: this path continues to use
`resolve_lib_type_by_name` plus the existing `Intl.*` namespace fallback for
the proven `CollatorOptions` slice.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 39 | 30 | 984 | 2 | 434 | 38 |
| monorepo-006 after | 37 | 28 | 984 | 2 | 434 | 36 |
| delta | -2 | -2 | 0 | 0 | 0 | -2 |

Diagnostics count is unchanged (`10,198` on both runs).

## Miss Classification

| Bucket | Baseline | After |
| --- | ---: | ---: |
| `target_declaration_files` | 30 | 28 |
| `target_source_files` | 0 | 0 |
| `by_kind.type_alias` | 16 | 16 |
| `by_kind.interface` | 14 | 12 |

The remaining declaration-file residue is now 28 misses: 16 type aliases and 12
interfaces.

## Decision

1. Keep the value-bearing interface allowlist for the proven core slice above.
2. Keep alias symbols on fallback paths; alias residue remains the larger
   unresolved bucket.
3. Next residue work should target the 12 remaining interface misses where
   namespace-qualified resolution or declaration-name matching still blocks the
   direct path.
