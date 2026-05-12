# perf(docs): T0.4 attribution run + decision record

- **Date**: 2026-05-10
- **Branch**: `perf/t0.4-attribution-decision-record-2026-05-10`
- **PR**: #4952
- **Status**: shipped
- **Workstream**: `PERFORMANCE_PLAN.md` Tier 0 — T0.4 / PR 3 (Tier 0 exit gate)

## Intent

Land the first checked-in attribution-mode phase split per
`PERFORMANCE_PLAN.md` §4.T0.4. The decision record names the next-tier
work that fresh data justifies, instead of acting on the historical
890s `large-ts-repo` figure.

## Approach

1. Run the perf-tools `tsz` binary in attribution mode
   (`TSZ_PERF_COUNTERS=1`) against the generated scale-cliff
   `monorepo-{001..006}` fixtures. `large-ts-repo` was explicitly
   deferred in this round because the host still hit OOM/stack-overflow
   behavior on that corpus.
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

## Findings

- Fixtures: `scripts/bench/scale-cliff/fixtures/monorepo-{001..006}`
  at `tsz` commit `ba1db057bb`
- Wall / RSS peak:
  - monorepo-001: 0.40s / 196 MB
  - monorepo-002: 0.73s / 943 MB
  - monorepo-003: 11.30s / 3940 MB
  - monorepo-004: 11.06s / 3955 MB
  - monorepo-005: 11.26s / 4199 MB
  - monorepo-006: 11.58s / 4008 MB
- Phase split at the cliff (`monorepo-003..006`):
  - `check`: ~85%
  - `parse_bind`: ~12.5%
  - `io_read`: ~2-3%
- Top counter buckets (`monorepo-006`):
  - `checker.with_parent_cache_constructed = 6738` (~files * 1.28)
  - `delegate.calls = 1148`, `delegate.cache_hits_cross_file = 0`
  - `interner.string_intern_calls = 7,117,797`
- Chosen next tier: promote T2.2 (typed cross-file query cache hits)
  and T2.1 (checker lifetime split), defer T2.0/T2.4.

## Out of scope (follow-up)

- Wiring the missing counters (`interner.intern_calls`,
  `resolver.is_file_calls`, lock-wait histogram). Tier 2.0/2.4 own
  those.
- Making the bench harness itself emit JSON. T0.2/T0.3 surface is
  already wired into `tsz`; the harness can shell out.
