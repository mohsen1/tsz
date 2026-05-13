# 2026-05-13 — `compute_type_of_symbol` Interface Simple-Local-Object Hit Counter (monorepo-006)

Follow-up to `2026-05-13-compute-type-of-symbol-interface-simple-local-object-fastpath.md`.

Goal: expose a direct scalar counter for how many interface-symbol
`compute_type_of_symbol` calls return through the simple local-object shortcut.

## Change

Add `checker.compute_type_of_symbol_interface_simple_object_fastpath_hits` to
perf counters and wire it at the shortcut return site.

Wiring scope:

1. `PerfCounters` atomic field + recorder helper.
2. `CheckerCounters` JSON snapshot field and text dump line.
3. JSON shape/propagation tests.
4. Interface branch callsite in `compute_type_of_symbol`.

## Reproducer

| Item | Value |
| --- | --- |
| Raw artifact | `docs/plan/perf-runs/raw/2026-05-13-compute-type-of-symbol-interface-simple-local-object-hit-counter-monorepo-006.json` |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --release` |
| Fixture | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Command | `.target/release/tsz --noEmit -p <fixture>/tsconfig.json --extendedDiagnostics --pretty false` |

## Result

From the run:

- diagnostics: `10,198`
- `compute_type_of_symbol.kind.interface`: `24,796`
- `checker.compute_type_of_symbol_interface_simple_object_fastpath_hits`: `24,760`

Derived ratio:

- simple-local-object shortcut hit rate among interface calls:
  `24,760 / 24,796 = 99.85%`

This confirms the shortcut is not just effective for time reduction but also
stable at near-total coverage for interface call volume on monorepo-006.

## Safety correction

This count came from the original broad shortcut. The branch was later narrowed
after targeted unit failures showed that empty interfaces and non-primitive
member annotations need the normal hybrid type-lowering path. Keep the counter
field, but do not treat the `24,760` hit count as the current guarded-branch
baseline until monorepo-006 is remeasured.

Guarded rerun is now recorded at:
`2026-05-13-compute-type-of-symbol-interface-simple-local-object-guarded-rerun.md`.
On that rerun, `checker.compute_type_of_symbol_interface_simple_object_fastpath_hits = 0`.

## Decision

1. Keep this scalar counter in the checker section as the primary guardrail for
   future interface-lowering edits.
2. Use it alongside interface callsite buckets when evaluating whether a future
   root-demand optimization is reducing work by volume or by per-call cost.
