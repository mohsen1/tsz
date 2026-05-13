# 2026-05-13 — `compute_type_of_symbol` Interface Fast Path (monorepo-006)

Follow-up to the same-day interface-heavy attribution lane on monorepo-006.

Goal: reduce interface-branch overhead in `compute_type_of_symbol` without
changing diagnostics or counter bucket distribution.

## Change

`compute_type_of_symbol` interface branch now skips work on simple local
interfaces:

1. Skip computed-property precompute maps when no computed property names exist.
2. Skip member type-reference prewarm scan on single-declaration local symbols.
3. Skip local heritage merge when no local `extends` clause exists.

Cross-file merge paths and lib resolution behavior are unchanged.

## Reproducer

| Item | Value |
| --- | --- |
| New raw | `docs/plan/perf-runs/raw/2026-05-13-compute-type-of-symbol-interface-fastpath-monorepo-006.json` |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --release` |
| Fixture | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Command | `.target/release/tsz --noEmit -p <fixture>/tsconfig.json --extendedDiagnostics --pretty false` |

Note: first post-change run had cold I/O (`I/O Read = 1.19s`, `Total = 84.17s`).
The recorded comparison uses the immediately following warm run (`I/O Read = 0.32s`).

## Result

Timing deltas vs the pre-change attribution baseline for the same fixture:

- total: `82.36s -> 81.25s` (`-1.11s`, `-1.35%`)
- check: `80.69s -> 79.60s` (`-1.09s`, `-1.35%`)
- parse+bind: `1.19s -> 1.14s` (`-0.05s`)
- I/O read: `0.29s -> 0.32s` (`+0.03s`)

Correctness/counter stability:

- diagnostics unchanged: `10,198`
- `compute_type_of_symbol.total_calls` unchanged: `26,370`
- `compute_type_of_symbol.cache_hits` unchanged: `252,026`
- source/kind bucket counts unchanged (including `interface = 24,781`)

## Decision

1. Keep this fast path; it gives a small but measurable win with unchanged output.
2. Keep interface lane active, but shift next step to call-site-driven reduction
   of interface symbol cold calls (bucket volume is still `94.0%` interface).
