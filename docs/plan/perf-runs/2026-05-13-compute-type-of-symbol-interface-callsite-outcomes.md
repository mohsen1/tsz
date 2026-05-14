# 2026-05-13 — `compute_type_of_symbol` Interface Call-site Outcomes (monorepo-006)

Follow-up to `2026-05-13-compute-type-of-symbol-interface-fastpath-outcomes.md`.

Goal: classify where interface-symbol `compute_type_of_symbol` calls come from
in the symbol-resolution stack, so the next reduction targets the dominant
caller shape.

## Reproducer

| Item | Value |
| --- | --- |
| Raw artifact | `docs/plan/perf-runs/raw/2026-05-13-compute-type-of-symbol-interface-callsite-outcomes-monorepo-006.json` |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --release` |
| Fixture | `/Users/mohsen/code/tsz/scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Command | `.target/release/tsz --noEmit -p <fixture>/tsconfig.json --extendedDiagnostics --pretty false` |

## Result

From the run:

- diagnostics: `10,198`
- `compute_type_of_symbol.kind.interface`: `24,796`

Interface call-site outcomes:

- `root`: `24,782` (`99.94%`)
- `parent_interface`: `14` (`0.06%`)
- all other parent-kind buckets: `0`

## Decision

1. Keep interface-path micro-tuning de-prioritized: almost all interface calls
   are root calls, not nested dependency chains.
2. Target the next PR at reducing root interface demand (top-level
   `get_type_of_symbol` requests), not interface-to-interface recursion.
