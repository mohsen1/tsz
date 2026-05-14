# 2026-05-13 - Actual-Lib Alias Proof Result Attribution

Branch-local attribution run on
`perf/actual-lib-alias-proof-result-20260513`
(`aca397eb1b99f47969a1d96fc685a27e58bbc01b`). This measures the
`direct_actual_lib_alias_body_outcomes` counter after the behavior-neutral
proof-result plumbing, before any generic alias widening.

## Reproducer

| Item | Value |
| --- | --- |
| branch | `perf/actual-lib-alias-proof-result-20260513` |
| commit | `aca397eb1b99f47969a1d96fc685a27e58bbc01b` |
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
  --diagnostics-json /tmp/tsz-perf-alias-proof-result-20260513/monorepo-006-diag.json \
  --perf-counters-json /tmp/tsz-perf-alias-proof-result-20260513/monorepo-006-pc.json
```

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

Raw JSON:

- `docs/plan/perf-runs/raw/2026-05-13-actual-lib-alias-proof-result-monorepo-006-diag.json`
- `docs/plan/perf-runs/raw/2026-05-13-actual-lib-alias-proof-result-monorepo-006-pc.json`

## Headline Counters

| Fixture | with_parent_cache | `DelegateCrossArenaSymbol` children | delegate calls | lib hits | cross-file hits | misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| monorepo-006 proof-result branch | 29 | 26 | 977 | 1 | 434 | 28 |

Diagnostics count remains `10,198`, matching the prior alias-body query run.

## Alias-Body Outcomes

| Outcome | Count |
| --- | ---: |
| `success` | 2 |
| `name_not_admitted` | 14 |
| `not_type_alias` | 0 |
| `value_merge` | 0 |
| `unproven_actual_lib_declarations` | 0 |
| `missing_resolver_type` | 0 |
| `resolver_not_lazy_def` | 0 |
| `missing_definition` | 0 |
| `non_type_alias_definition` | 0 |
| `missing_body` | 0 |
| `generic_alias` | 0 |

The two successes are the admitted decorator metadata aliases. The remaining
alias misses stop at the conservative admission gate, so this counter confirms
that the next generic-aware slice must separate resolver/body proof from
admission before it can classify generic utility aliases by body shape.

## Remaining Alias Residue

The declaration-file miss residue table still reports these type-alias rows:

| Alias | Count | Target file |
| --- | ---: | --- |
| `FlatArray` | 2 | `lib.es2019.array.d.ts` |
| `IteratorResult` | 2 | `lib.es2015.iterable.d.ts` |
| `Record` | 2 | `lib.es5.d.ts` |
| `LocalesArgument` | 1 | `lib.es2020.intl.d.ts` |
| `NumberFormatOptionsCurrencyDisplay` | 1 | `lib.es5.d.ts` |
| `NumberFormatOptionsStyle` | 1 | `lib.es5.d.ts` |
| `NumberFormatOptionsUseGrouping` | 1 | `lib.es5.d.ts` |
| `Partial` | 1 | `lib.es5.d.ts` |
| `PropertyKey` | 1 | `lib.es5.d.ts` |
| `Readonly` | 1 | `lib.es5.d.ts` |
| `UnicodeBCP47LocaleIdentifier` | 1 | `lib.es2020.intl.d.ts` |

## Decision

Keep the proof-result slice behavior-neutral. The next behavior-changing alias
work should not widen the current name gate directly. It should first make the
typed proof usable independently from admission, then apply a conformance-backed
generic alias family through a real substitution/application path.
