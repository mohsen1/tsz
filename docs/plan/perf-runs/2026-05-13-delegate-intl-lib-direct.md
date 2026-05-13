# 2026-05-13 - DelegateCrossArenaSymbol Intl-Lib Direct

Attribution-mode follow-up for the remaining declaration-file
`DelegateCrossArenaSymbol` residue. This run validates a namespace-qualified
actual-lib interface path for `Intl.CollatorOptions`.

## Reproducer

| Item | Value |
| --- | --- |
| `tsz` code commit | local branch before final PR commit |
| `origin/main` base | `a929325492` |
| `tsz` build | `CARGO_TARGET_DIR=.target cargo build -p tsz-cli --bin tsz --features perf-tools --release` |
| Fixture generator | `scripts/bench/scale-cliff/generate-fixtures.sh` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Timing feature | `tsz-common/perf-counters-timing` ON via `perf-tools` |
| Machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- `docs/plan/perf-runs/raw/2026-05-13-delegate-intl-lib-direct-monorepo-006-diag.json`
- `docs/plan/perf-runs/raw/2026-05-13-delegate-intl-lib-direct-monorepo-006-pc.json`

The synthetic fixture still emits diagnostics, so `tsz` exits with code `2`.
The diagnostics and perf-counter JSON files are still written and are the
artifacts used below.

## Change

`direct_actual_lib_symbol_type` now falls back from global lib-name resolution
to an `Intl` namespace export lookup for the single proven
`CollatorOptions` interface. The path still requires the target to be an
actual bundled-lib symbol, rejects values, aliases, type aliases, multi-decl
symbols, and non-builtin declarations, and only lowers heritage-free
interfaces by symbol through the existing lib declaration lowering helpers.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 before (#6260) | 55 | 40 | 991 | 3 | 434 | 54 |
| monorepo-006 after | 54 | 39 | 991 | 3 | 434 | 53 |
| delta | -1 | -1 | 0 | 0 | 0 | -1 |

## Miss Classification

| Bucket | Before (#6260) | After |
| --- | ---: | ---: |
| `target_declaration_files` | 40 | 39 |
| `target_source_files` | 0 | 0 |
| `by_kind.type_alias` | 16 | 16 |
| `by_kind.interface` | 24 | 23 |

The remaining 39 misses include all 16 declaration-file type aliases plus 23
interfaces that still need merged-lib, namespace-qualified, or
conformance-backed proof. This PR only proves the single-declaration
namespace-export case; it deliberately does not relax the guards for `Locale`,
`NumberFormatOptions`, registry merges, core collection interfaces, or utility
type aliases.

## Phase Split

Attribution-mode wall time is not comparable to `tsgo` or timing-mode `tsz`.
Use these numbers only for phase dominance.

| Fixture | total s | check s | check % | diagnostics |
| --- | ---: | ---: | ---: | ---: |
| monorepo-006 | 80.18 | 78.55 | 98.0 | 10,198 |

## Decision

1. Keep namespace-qualified direct actual-lib lowering limited to
   `Intl.CollatorOptions` until merged `Intl` interfaces and aliases have
   targeted conformance coverage.
2. Treat the next declaration-file residue as 39 misses: 16 type aliases and
   23 interfaces.
