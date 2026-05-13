# 2026-05-13 - DelegateCrossArenaSymbol Actual-Lib Direct

Attribution-mode follow-up for #6243. This run validates a direct path for the
actual bundled-lib portion of the remaining declaration-file symbol-arena
delegations.

## Reproducer

| Item | Value |
| --- | --- |
| `tsz` code commit | local branch before PR publication |
| `origin/main` base | `a929325492` |
| `tsz` build | `CARGO_TARGET_DIR=.target cargo build -p tsz-cli --bin tsz --features perf-tools --release` |
| Fixture generator | `scripts/bench/scale-cliff/generate-fixtures.sh` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Timing feature | `tsz-common/perf-counters-timing` ON via `perf-tools` |
| Machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- `docs/plan/perf-runs/raw/2026-05-13-delegate-actual-lib-direct-monorepo-006-diag.json`
- `docs/plan/perf-runs/raw/2026-05-13-delegate-actual-lib-direct-monorepo-006-pc.json`

The synthetic fixture still emits diagnostics, so `tsz` exits with code `2`.
The diagnostics and perf-counter JSON files are still written and are the
artifacts used below.

## Change

`delegate_cross_arena_symbol_resolution` now checks for actual bundled-lib
symbol-arena targets before constructing a child checker. The direct path is
limited to non-DOM/non-webworker bundled standard-lib declaration arenas,
requires every symbol declaration to stay in matching bundled-lib arenas,
rejects value-merged symbols and type aliases, and reuses
`resolve_lib_type_by_name` so canonical lib `DefId` registration, lazy aliases,
and merged lib declaration behavior stay inside the existing lib resolver.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 before (#6243) | 56 | 41 | 991 | 3 | 434 | 55 |
| monorepo-006 current main (#6286) | 55 | 40 | 991 | 3 | 434 | 54 |
| monorepo-006 after | 40 | 31 | 984 | 2 | 434 | 39 |
| delta vs current main | -15 | -9 | -7 | -1 | 0 | -15 |

The remaining child-checker count is higher than the first broad prototype
because this version leaves lib type aliases, DOM/webworker surfaces,
value-merged symbols, and non-builtin declaration merges on the established
fallback paths.

## Miss Classification

| Bucket | Before (#6243) | After |
| --- | ---: | ---: |
| `target_declaration_files` | 41 | 31 |
| `target_source_files` | 0 | 0 |
| `by_kind.type_alias` | 16 | 16 |
| `by_kind.interface` | 25 | 15 |

The remaining 31 misses include all 16 declaration-file type aliases plus 15
interfaces that still need either namespace-qualified proof, merged-lib
declaration proof, or targeted conformance coverage. The rejected alias slice is
deliberate: broader prototypes changed observable diagnostics for utility
aliases such as `FlatArray`, `Readonly`, and `Record`.

## Phase Split

Attribution-mode wall time is not comparable to `tsgo` or timing-mode `tsz`.
Use these numbers only for phase dominance.

| Fixture | total s | check s | check % | diagnostics |
| --- | ---: | ---: | ---: | ---: |
| monorepo-006 | 81.83 | 80.11 | 97.9 | 10,198 |

## Decision

1. Keep the non-DOM/non-webworker actual-lib direct path. It removes another
   interface portion of the declaration-file child-checker residue while
   preserving the existing lib resolver as the canonical lowering
   implementation.
2. Do not broaden this PR to lib type aliases, DOM/webworker surfaces, or
   value-merged symbols. They need separate proof and targeted tests.
3. The next residue target is the 31 remaining declaration-file misses, split
   into alias and interface slices rather than handled as a single shortcut.
