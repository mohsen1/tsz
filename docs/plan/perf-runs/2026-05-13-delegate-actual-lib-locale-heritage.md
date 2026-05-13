# 2026-05-13 - DelegateCrossArenaSymbol Intl.Locale Heritage Follow-up

Attribution-mode follow-up on top of
`7251b51c78` (`perf(checker): admit Intl sign-display registry interface`).

## Reproducer

| Item | Value |
| --- | --- |
| baseline commit | `7251b51c78` |
| after branch | `codex/perf-goal-next-20260513-replay` (this slice) |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| fixture path | `/private/tmp/tsz-bench-fixtures/monorepo-006` |
| counter mode | `TSZ_PERF_COUNTERS=1` |
| machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- Baseline:
  `docs/plan/perf-runs/raw/2026-05-13-delegate-actual-lib-intl-sign-display-registry-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-delegate-actual-lib-intl-sign-display-registry-after-monorepo-006-pc.json`
- After:
  `docs/plan/perf-runs/raw/2026-05-13-delegate-actual-lib-locale-heritage-after-monorepo-006-diag.json`
  `docs/plan/perf-runs/raw/2026-05-13-delegate-actual-lib-locale-heritage-after-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

## Change

Extend the existing narrow direct actual-lib path with:

- `Locale` admission in the Intl namespace-qualified direct interface allowlist.
- A bounded heritage exception (`Intl.Locale`, `Iterator`) in
  `resolve_lib_interface_type_by_cache_name`.
- A guarded `Iterator` symbol fallback when parameterized lib resolution returns
  `None`, preserving type-parameter plumbing.

Alias handling remains unchanged; utility alias rows still stay on fallback.

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` children | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 baseline | 17 | 17 | 976 | 1 | 434 | 17 |
| monorepo-006 after | 16 | 16 | 976 | 1 | 434 | 16 |
| delta | -1 | -1 | 0 | 0 | 0 | -1 |

Diagnostics count is unchanged (`10,198` on both runs).

## Miss Classification

| Bucket | Baseline | After |
| --- | ---: | ---: |
| `target_declaration_files` | 17 | 16 |
| `target_source_files` | 0 | 0 |
| `by_kind.type_alias` | 15 | 15 |
| `by_kind.interface` | 2 | 1 |

Declaration-file residue row removed:

- `Locale`

Remaining declaration-file interface residue:

- `Iterator`

## Decision

Keep this focused follow-up. It removes another declaration-file interface
delegate and preserves diagnostics without widening alias admission.
