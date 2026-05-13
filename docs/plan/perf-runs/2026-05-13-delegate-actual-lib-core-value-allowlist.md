# 2026-05-13 - DelegateCrossArenaSymbol Actual-Lib Core Value-Interface Allowlist

Attribution-mode follow-up on top of the merged-declaration actual-lib direct
path. This run admits a narrowly scoped value-bearing interface set through the
same direct gateway.

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `8427e11638` (`Merge remote-tracking branch 'origin/main' into codex/perf-actual-lib-allowlist-expansion-20260513`) |
| after commit | `22b0f2c891` (`perf(checker): admit selected value-bearing actual-lib interfaces`) |
| baseline build | `CARGO_TARGET_DIR=/tmp/tsz-target-corevalue-baseline cargo build -p tsz-cli --bin tsz --features perf-tools --release` |
| after build | `CARGO_TARGET_DIR=/tmp/tsz-target-corevalue-after cargo build -p tsz-cli --bin tsz --features perf-tools --release` |
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

The resolver policy is unchanged: allowlisted names in the
`should_resolve_actual_lib_interface_with_params` set continue to use
`resolve_lib_type_with_params`, and non-listed names keep the existing
`resolve_lib_type_by_name` path.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 26 | 26 | 971 | 1 | 434 | 26 |
| monorepo-006 after | 22 | 22 | 971 | 1 | 434 | 22 |
| delta | -4 | -4 | 0 | 0 | 0 | -4 |

Diagnostics count is unchanged (`10,198` on both runs).

## Miss Classification

| Bucket | Baseline | After |
| --- | ---: | ---: |
| `target_declaration_files` | 26 | 22 |
| `target_source_files` | 0 | 0 |
| `by_kind.type_alias` | 15 | 15 |
| `by_kind.interface` | 11 | 7 |

The remaining declaration-file residue is now 22 misses: 15 type aliases and 7
interfaces.

## Decision

1. Keep the value-bearing interface allowlist for the proven core slice above.
2. Keep alias symbols on fallback paths; alias residue remains the larger
   unresolved bucket.
3. Next residue work should target the 7 remaining interface misses where
   `resolve_lib_type_with_params` still returns `None` (e.g. namespace/
   registry-shaped entries) or declaration-name matching excludes the symbol.
