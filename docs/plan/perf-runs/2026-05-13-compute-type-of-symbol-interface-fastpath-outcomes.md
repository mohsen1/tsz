# 2026-05-13 — `compute_type_of_symbol` Interface Fast-Path Outcomes (monorepo-006)

Follow-up to `2026-05-13-compute-type-of-symbol-interface-fastpath.md`.

Goal: measure how often each interface fast-path combination is used so the
next optimization targets real residual work.

## Reproducer

| Item | Value |
| --- | --- |
| Raw artifact | `docs/plan/perf-runs/raw/2026-05-13-compute-type-of-symbol-interface-fastpath-outcomes-monorepo-006.json` |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --release` |
| Fixture | `/Users/mohsen/code/tsz/scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Command | `.target/release/tsz --noEmit -p <fixture>/tsconfig.json --extendedDiagnostics --pretty false` |

## Result

Key counters from the run:

- diagnostics: `10,198`
- `compute_type_of_symbol.total_calls`: `26,379`
- `compute_type_of_symbol.kind.interface`: `24,796`

Interface fast-path outcome split:

- `skip_all_three`: `24,767` (`99.88%` of interface calls)
- `skip_computed_name_map_and_local_heritage_merge`: `16` (`0.06%`)
- `skip_computed_name_map`: `1`
- `full_path`: `1`
- all other buckets: `0`

## Decision

1. Keep the fast-path gates as-is; they are already firing on nearly all
   interface calls.
2. Do not spend the next PR on further precompute/prewarm/heritage-gate tuning.
3. Move the next lane to call-site-driven reduction of interface cold-call
   volume (or interface-body lowering cost), because the gate residual is tiny.
