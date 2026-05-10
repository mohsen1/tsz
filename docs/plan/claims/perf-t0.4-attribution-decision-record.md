# perf(docs): T0.4 attribution run + decision record

- **Date**: 2026-05-10
- **Branch**: `perf/t0.4-attribution-decision-record-2026-05-10`
- **PR**: TBD
- **Status**: claim
- **Workstream**: `PERFORMANCE_PLAN.md` Tier 0 — T0.4 / PR 3 (Tier 0 exit gate)

## Intent

Land the first checked-in attribution-mode phase split per
`PERFORMANCE_PLAN.md` §4.T0.4. The decision record names the next-tier
work that fresh data justifies, instead of acting on the historical
890s `large-ts-repo` figure.

## Approach

1. Run the perf-tools `tsz` binary in attribution mode
   (`TSZ_PERF_COUNTERS=1`) against a representative slice of
   `large-ts-repo` and at least one scale-cliff `monorepo-NNN`
   fixture if the generator is fast enough to include in this PR.
2. Capture both JSON outputs:
   - `--diagnostics-json` (T0.2 schema)
   - `--perf-counters-json` (T0.3 schema)
3. Check in:
   - `docs/plan/perf-runs/2026-05-10-scale-cliff-summary.md` —
     short narrative + phase split table + chosen next tier.
   - `docs/plan/perf-runs/raw/` — committed copies of the JSON used
     in the summary (small enough to track in git; we explicitly
     do not push raw JSON to GCS for this first run).
4. Update `PERFORMANCE_PLAN.md` §2 status table for T0.4 and §4 exit
   matrix with the chosen next tier.

## Findings (to fill)

- Fixture: `<path>` at commit `<sha>`
- Wall: `<seconds>`
- RSS peak: `<MB>`
- Phase split (top 5):
  - …
- Top counter buckets:
  - …
- Chosen next tier: T2.0 / T2.1 / T2.2 / T2.4 / pause for sampling

## Out of scope (follow-up)

- Wiring the missing counters (`interner.intern_calls`,
  `resolver.is_file_calls`, lock-wait histogram). Tier 2.0/2.4 own
  those.
- Making the bench harness itself emit JSON. T0.2/T0.3 surface is
  already wired into `tsz`; the harness can shell out.
