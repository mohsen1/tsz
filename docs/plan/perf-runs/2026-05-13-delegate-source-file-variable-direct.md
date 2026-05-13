# 2026-05-13 - DelegateCrossArenaSymbol Source-File Variable Direct

Attribution-mode follow-up for #6231. This run validates a direct typed query
for the source-file variable subset left after direct source-file interface
lowering.

## Reproducer

| Item | Value |
| --- | --- |
| `tsz` code commit | local branch before PR publication |
| `origin/main` base | `a247c6bd52` |
| `tsz` build | `CARGO_TARGET_DIR=.target cargo build -p tsz-cli --bin tsz --features perf-tools --release` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Timing feature | `tsz-common/perf-counters-timing` ON via `perf-tools` |
| Machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- `docs/plan/perf-runs/raw/2026-05-13-delegate-source-file-variable-direct-monorepo-006-diag.json`
- `docs/plan/perf-runs/raw/2026-05-13-delegate-source-file-variable-direct-monorepo-006-pc.json`

The synthetic fixture still emits diagnostics, so `tsz` exits with code `2`.
The diagnostics and perf-counter JSON files are still written and are the
artifacts used below.

## Change

The source-file direct query now handles annotated variables after the stable
source-file symbol-arena proof passes. The direct variable path accepts:

- annotations that are scope-independent by themselves, or
- a type reference to a same-file interface accepted by the source-file direct
  interface query.

For the same-file interface case, the variable type remains a lazy interface
type. The direct query lowers the interface body to populate the shared
`DefinitionStore`, then returns `Lazy(DefId)` for the variable annotation.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 before (#6231) | 307 | 292 | 941 | 3 | 385 | 306 |
| monorepo-006 after | 56 | 41 | 991 | 3 | 434 | 55 |
| delta | -251 | -251 | +50 | 0 | +49 | -251 |

The extra delegate calls come from direct variable annotations asking the same
source-file interface query to populate interface bodies. Those calls do not
create child checkers.

## Miss Classification

| Bucket | Before (#6231) | After |
| --- | ---: | ---: |
| `by_kind.variable` | 251 | 0 |
| `target_source_files` | 251 | 0 |
| `target_declaration_files` | 41 | 41 |
| `by_kind.type_alias` | 16 | 16 |
| `by_kind.interface` | 25 | 25 |

After this PR there is no measured source-file `DelegateCrossArenaSymbol`
residue on monorepo-006. The remaining symbol-arena child checkers all target
declaration files.

## Phase Split

Attribution-mode wall time is not comparable to `tsgo` or timing-mode `tsz`.
Use these numbers only for phase dominance.

| Fixture | total s | check s | check % | diagnostics |
| --- | ---: | ---: | ---: | ---: |
| monorepo-006 | 84.80 | 83.08 | 97.9 | 10,198 |

## Decision

1. Keep the direct source-file variable query. It removes the measured
   source-file variable child-checker residue on monorepo-006 while preserving
   lazy interface annotation semantics.
2. Do not broaden variable annotation handling to arbitrary type aliases or
   target-file name resolution without a separate proof.
3. The next `DelegateCrossArenaSymbol` target is the remaining 41
   declaration-file symbol-arena child-checker constructions.

