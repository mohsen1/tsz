# 2026-05-13 - DelegateCrossArenaSymbol Actual-Lib Direct

Attribution-mode follow-up for #6243. This run validates a direct path for the
actual bundled-lib portion of the remaining declaration-file symbol-arena
delegations.

## Reproducer

| Item | Value |
| --- | --- |
| `tsz` code commit | local branch before PR publication |
| `origin/main` base | `774e86bd1c` |
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
limited to a small allowlist of standard-lib option/registry interfaces, rejects
type aliases, rejects symbols with non-builtin declarations, and reuses
`resolve_lib_type_by_name` so interface heritage and merged lib declaration
behavior stay inside the existing lib resolver.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 before (#6243) | 56 | 41 | 991 | 3 | 434 | 55 |
| monorepo-006 after | 55 | 40 | 991 | 3 | 434 | 54 |
| delta | -1 | -1 | 0 | 0 | 0 | -1 |

The `DelegateCrossArenaSymbol` reduction is smaller than the total
`with_parent_cache` reduction because the direct lib path also avoids recursive
child-checker work under the delegated lib resolution.

## Miss Classification

| Bucket | Before (#6243) | After |
| --- | ---: | ---: |
| `target_declaration_files` | 41 | 40 |
| `target_source_files` | 0 | 0 |
| `by_kind.type_alias` | 16 | 16 |
| `by_kind.interface` | 25 | 24 |

The remaining 40 misses include all 16 declaration-file type aliases plus 24
interfaces that still need either namespace-qualified proof, merged-lib
declaration proof, or targeted conformance coverage. The rejected alias and
core-interface slices are deliberate: broader prototypes changed observable
diagnostics for utility aliases such as `FlatArray`, `Readonly`, and `Record`,
and regressed conformance around recursive/call-signature lib identity.

## Phase Split

Attribution-mode wall time is not comparable to `tsgo` or timing-mode `tsz`.
Use these numbers only for phase dominance.

| Fixture | total s | check s | check % | diagnostics |
| --- | ---: | ---: | ---: | ---: |
| monorepo-006 | 94.28 | 92.79 | 98.4 | 10,198 |

## Decision

1. Keep only the conservative option/registry interface allowlist. It proves the
   direct actual-lib gateway shape while preserving the existing lib resolver as
   the canonical lowering implementation.
2. Do not broaden this PR to type aliases, namespace declarations, or core lib
   identity interfaces. The rejected prototypes changed diagnostic behavior or
   conformance; the remaining 40 misses need separate proof and targeted tests.
