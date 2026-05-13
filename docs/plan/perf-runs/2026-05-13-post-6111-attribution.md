# 2026-05-13 — Post-#6111 Attribution Refresh

Draft decision record for the next scale-cliff attribution run after #6111
landed the first source-file symbol arena cross-file cache path.

## Reproducer

| Item | Value |
| --- | --- |
| `tsz` commit | TBD |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --features perf-tools --release` |
| Fixture generator | `scripts/bench/scale-cliff/generate-fixtures.sh --clean` |
| Fixture path | `scripts/bench/scale-cliff/fixtures/monorepo-{001..006}` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |

## Expected Question

Now that `DelegateCrossArenaSymbol` source-file hits can enter the canonical
`SymbolType` cache bucket, identify the next load-bearing child-checker reason
or cache miss blocker from fresh counters on `monorepo-006`.

