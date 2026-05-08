# perf(bench): gate large-ts-repo local fallback behind TSZ_BENCH_ALLOW_LOCAL_FIXTURE

- **Date**: 2026-05-08
- **Branch**: `perf/bench-fixture-local-opt-in`
- **PR**: #4699
- **Status**: ready
- **Workstream**: PERFORMANCE_PLAN T0.1 (Tier 0 — Measurement and fixture correctness)

## Intent

`scripts/bench/bench-vs-tsgo.sh:103-110` previously silently fell back to
`${HOME}/code/large-ts-repo` whenever that directory existed, before the
pinned external clone. Any developer machine with a local checkout could
quietly contaminate PR-quality numbers depending on which commit the
local clone happened to be on.

Per `docs/plan/PERFORMANCE_PLAN.md` §3.5.1 (Amendment 1, 2026-05-08), the
local fallback is now opt-in via `TSZ_BENCH_ALLOW_LOCAL_FIXTURE=1`. The
default is the pinned external clone at `LARGE_TS_REF`. Explicit
`LARGE_TS_DIR=...` continues to win over both, unchanged.

This is the smallest and lowest-risk Tier 0 PR; it gates the rest of the
T0/T1/T2 work because every subsequent perf number must be reproducible.

## Files Touched

- `scripts/bench/bench-vs-tsgo.sh` (~6 LOC change at line 103)

## Verification

- `bash -n scripts/bench/bench-vs-tsgo.sh` (syntax OK).
- Probe script exercising all four resolution branches:
  - default (no env) → pinned external clone.
  - opt-in env without a local clone → pinned external clone.
  - opt-in env with a local clone → local clone.
  - explicit `LARGE_TS_DIR=...` → that value, regardless of opt-in.
  - legacy implicit fallback (env unset, local clone present) → pinned
    external clone (the change we wanted).
