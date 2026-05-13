# Delegate Actual-Lib Value Interfaces Main Slice

Date: 2026-05-13

Branch: `codex/perf-actual-lib-value-interfaces-main-20260513`

Base: `origin/main` at `8bfa2b11db`

## Change

Admit a narrow set of value-bearing bundled-lib interfaces through
`direct_actual_lib_symbol_type`:

- `Function`
- `Object`
- `RegExp`

These names are omitted from the broader generic/type-alias work. Generic
utility aliases and assignability-sensitive aliases remain on the existing
fallback path.

## Command

```sh
CARGO_TARGET_DIR=.target-perf-refresh-main-20260513 cargo build -p tsz-cli --release --features perf-tools
CARGO_TARGET_DIR=.target-perf-refresh-20260513 cargo build -p tsz-cli --release --features perf-tools

TSZ_PERF_COUNTERS=1 .target-perf-refresh-20260513/release/tsz \
  --project scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json \
  --pretty false --noEmit \
  --diagnostics-json .ci-logs/perf-value-interfaces-rebased-20260513/diag.json \
  --perf-counters-json .ci-logs/perf-value-interfaces-rebased-20260513/pc.json

.target-perf-refresh-main-20260513/release/tsz \
  --project scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json \
  --pretty false --noEmit \
  --diagnostics-json .ci-logs/perf-timing-main-20260513/diag.json

.target-perf-refresh-20260513/release/tsz \
  --project scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json \
  --pretty false --noEmit \
  --diagnostics-json .ci-logs/perf-timing-branch-20260513/diag.json
```

The fixture exits with diagnostics (`exit=2`), as expected for the current
scale-cliff fixture shape; diagnostics count is compared below.

## Result

| Counter | Before | After |
| --- | ---: | ---: |
| Diagnostics | 10,198 | 10,198 |
| `checker.with_parent_cache_constructed` | 29 | 26 |
| `DelegateCrossArenaSymbol` | 26 | 23 |
| `delegate.misses` | 28 | 25 |
| `delegate.calls` | 977 | 977 |
| `delegate.cache_hits_cross_file` | 434 | 434 |

Removed declaration-file residue rows:

- `Function`
- `Object`
- `RegExp`

Counter-free timing-mode comparison on the same fixture:

| Timing | Main | Branch |
| --- | ---: | ---: |
| Diagnostics | 10,198 | 10,198 |
| `check_ms` | 166,369.995 | 158,218.144875 |
| `total_ms` | 170,182.516291 | 160,254.267208 |
| `rss_peak_bytes` | 3,018,948,608 | 3,019,358,208 |
