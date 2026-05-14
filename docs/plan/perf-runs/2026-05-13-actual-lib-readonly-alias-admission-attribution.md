# 2026-05-13 - Actual-Lib Readonly Alias Admission Attribution

Branch-local attribution run on
`codex/perf-actual-lib-readonly-alias-admission-20260513`, stacked on
`perf/actual-lib-alias-proof-admission-20260513`. This measures admitting only
the proven `Readonly<T>` actual-lib alias through the direct alias-body path.

## Reproducer

| Item | Value |
| --- | --- |
| branch | `codex/perf-actual-lib-readonly-alias-admission-20260513` |
| base branch | `perf/actual-lib-alias-proof-admission-20260513` |
| `tsz` build | `cargo build -p tsz-cli --release --features perf-tools` |
| fixture generation | `scripts/bench/scale-cliff/generate-fixtures.sh` |
| fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| counter mode | `TSZ_PERF_COUNTERS=1` |
| machine | macOS Darwin arm64 |

Command:

```bash
TSZ_PERF_COUNTERS=1 ./.target/release/tsz \
  --project scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json \
  --pretty false --noEmit \
  --diagnostics-json docs/plan/perf-runs/raw/2026-05-13-actual-lib-readonly-alias-admission-monorepo-006-diag.json \
  --perf-counters-json docs/plan/perf-runs/raw/2026-05-13-actual-lib-readonly-alias-admission-monorepo-006-pc.json
```

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

Raw JSON:

- `docs/plan/perf-runs/raw/2026-05-13-actual-lib-readonly-alias-admission-baseline-monorepo-006-diag.json`
- `docs/plan/perf-runs/raw/2026-05-13-actual-lib-readonly-alias-admission-baseline-monorepo-006-pc.json`
- `docs/plan/perf-runs/raw/2026-05-13-actual-lib-readonly-alias-admission-monorepo-006-diag.json`
- `docs/plan/perf-runs/raw/2026-05-13-actual-lib-readonly-alias-admission-monorepo-006-pc.json`

## Headline Counters

Both rows use the same regenerated local monorepo-006 fixture with `5,337`
files and `10,198` diagnostics.

| Fixture | with_parent_cache | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: |
| proof/admission baseline | 29 | 977 | 1 | 434 | 28 |
| Readonly admitted | 28 | 977 | 1 | 434 | 27 |

This is attribution-mode data only. The after-run wall time happened to be
lower than the baseline rerun on this machine, but this claim is counter-gated
rather than a timing claim.

## Alias-Body Outcomes

| Outcome | Baseline | Readonly admitted |
| --- | ---: | ---: |
| `success` | 2 | 3 |
| `name_not_admitted` | 1 | 1 |
| `missing_resolver_type` | 5 | 5 |
| `generic_alias` | 8 | 7 |

All other alias-body outcome buckets remain `0`.

## Declaration-File Residue

The single `Readonly` declaration-file residue is removed:

| Residue | Baseline | Readonly admitted |
| --- | ---: | ---: |
| `Readonly` | 1 | 0 |

The remaining alias residue is still conservative: `FlatArray` (2),
`IteratorResult` (2), `Record` (2), `Partial` (1), `PropertyKey` (1),
`LocalesArgument` (1), `NumberFormatOptionsCurrencyDisplay` (1),
`NumberFormatOptionsStyle` (1), `NumberFormatOptionsUseGrouping` (1), and
`UnicodeBCP47LocaleIdentifier` (1).

## Decision

Keep `Readonly<T>` as the first behavior-changing generic alias admission
because it has direct proof/fallback parity coverage and removes one measured
`DelegateCrossArenaSymbol` miss. The next alias slice should target another
single proven residue, not a broad utility-alias allowlist.
