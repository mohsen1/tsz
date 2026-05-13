# 2026-05-13 - DelegateCrossArenaSymbol Iterator Value-Merged Actual-Lib Direct Path

Attribution-mode follow-up on top of `7f34be3a64` to reduce declaration-file
`DelegateCrossArenaSymbol` residue without reopening broad utility-alias
admission.

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `7f34be3a64` |
| after branch | `56518a492e` (this slice) |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| counter mode | `TSZ_PERF_COUNTERS=1` |
| machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-13-delegate-actual-lib-iterator-value-merged-baseline-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-delegate-actual-lib-iterator-value-merged-baseline-monorepo-006-pc.json`
- After:
  `docs/plan/perf-runs/raw/2026-05-13-delegate-actual-lib-iterator-value-merged-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-delegate-actual-lib-iterator-value-merged-after-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

## Change

`direct_actual_lib_symbol_type` now admits a narrow value-merged actual-lib
interface slice:

1. route `Iterator` and `IteratorObject` through
   `resolve_lib_type_with_params`, and
2. allow value-merged admission only for that iterator pair (other
   value-merged symbols still return `None` and stay on fallback).

This keeps the admission scope tight while proving that a value-merged lib
interface path can reduce declaration-file delegation residue safely.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` children | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 25 | 22 | 977 | 1 | 434 | 24 |
| monorepo-006 after | 20 | 20 | 974 | 1 | 434 | 20 |
| delta | -5 | -2 | -3 | 0 | 0 | -4 |

Diagnostics count is unchanged (`10,198` on both runs).

Counters-off timing is noisy on this fixture. Two local pairs against the same
release binaries produced:

| Run | baseline `check_ms` | after `check_ms` | delta |
| --- | ---: | ---: | ---: |
| baseline first | 143,940.9 | 153,760.9 | +9,820.0 |
| after first | 137,851.7 | 131,189.7 | -6,661.9 |

Do not use this slice to claim a wall-clock timing win by itself; the supported
claim is reduced direct-delegation residue with unchanged diagnostics.

## Miss Classification

| Bucket | Baseline | After |
| --- | ---: | ---: |
| `target_declaration_files` | 22 | 20 |
| `target_source_files` | 0 | 0 |
| `by_kind.type_alias` | 13 | 13 |
| `by_kind.interface` | 9 | 7 |

Declaration-file residue names removed by this slice:

- `IteratorObject`
- `Symbol`

`Iterator` remains at `count=1` and should be rechecked in the next residue
slice with name-level tracing at the fallback point.

## Decision

Keep this narrow iterator value-merged admission only if the merge gate accepts
the noisy timing profile. It reduces `DelegateCrossArenaSymbol` children and
misses with no diagnostic drift while avoiding broad value-merged interface
relaxation, but it should not be auto-merged on the attribution counters alone.
The next declaration-file slice should target remaining interfaces (`Function`,
`Object`, `RegExp`, `Iterator`) one family at a time with the same counter +
conformance gates.
