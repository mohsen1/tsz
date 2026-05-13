# 2026-05-13 - Actual-Lib Alias Proof Admission Attribution

Branch-local attribution run on
`perf/actual-lib-alias-proof-admission-20260513`
(`31d5f08984b3499c7a289da1e6e83dad67eec4e1`). This measures the
proof/admission split after moving the decorator allowlist behind actual-lib
alias body proof.

## Reproducer

| Item | Value |
| --- | --- |
| branch | `perf/actual-lib-alias-proof-admission-20260513` |
| commit | `31d5f08984b3499c7a289da1e6e83dad67eec4e1` |
| `tsz` build | `cargo build -p tsz-cli --release --features perf-tools` |
| fixture generation | `scripts/bench/scale-cliff/generate-fixtures.sh` |
| fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| counter mode | `TSZ_PERF_COUNTERS=1` |
| machine | macOS Darwin arm64 |

Command:

```bash
TSZ_PERF_COUNTERS=1 .target/release/tsz \
  --extendedDiagnostics --noEmit \
  -p scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json \
  --diagnostics-json /tmp/tsz-perf-alias-proof-admission-20260513/monorepo-006-diag.json \
  --perf-counters-json /tmp/tsz-perf-alias-proof-admission-20260513/monorepo-006-pc.json
```

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

Raw JSON:

- `docs/plan/perf-runs/raw/2026-05-13-actual-lib-alias-proof-admission-monorepo-006-diag.json`
- `docs/plan/perf-runs/raw/2026-05-13-actual-lib-alias-proof-admission-monorepo-006-pc.json`

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` children | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 proof/admission split | 29 | 26 | 977 | 1 | 434 | 28 |

Diagnostics count remains `10,198`. This slice is intentionally attribution
only; it should not change direct-return behavior.

## Alias-Body Outcomes

| Outcome | Proof-result branch | Proof/admission split |
| --- | ---: | ---: |
| `success` | 2 | 2 |
| `name_not_admitted` | 14 | 1 |
| `missing_resolver_type` | 0 | 5 |
| `generic_alias` | 0 | 8 |

All other alias-body outcome buckets remain `0`.

The split turns the remaining declaration-file alias residue into more useful
proof outcomes:

- `generic_alias = 8`: `FlatArray` (2), `IteratorResult` (2), `Record` (2),
  `Partial` (1), and `Readonly` (1).
- `missing_resolver_type = 5`: the remaining lib alias rows that are not
  resolved by the current direct name resolver.
- `name_not_admitted = 1`: `PropertyKey`, which remains deliberately on
  fallback because earlier conformance showed it is assignability-sensitive.

## Decision

Keep the proof/admission split as behavior-neutral plumbing. The next
behavior-changing alias PR should target one conformance-backed generic alias
family from the `generic_alias` bucket and apply it through real substitution,
instead of widening the direct-return admission gate by name.
