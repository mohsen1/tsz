# fix(audit): align perf T0.4 claim record and raw provenance artifacts

- **Date**: 2026-05-12
- **Branch**: `codex/review-audit-perf-docs-followup-20260512`
- **PR**: TBD
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

Close missed high-signal review comments on PR #4952 by making the T0.4 decision record internally consistent with what was actually measured, and by sanitizing/attributing checked-in raw diagnostics artifacts for portability and reproducibility.

## Files Touched

- `docs/plan/claims/perf-t0.4-attribution-decision-record.md`
- `docs/plan/perf-runs/2026-05-10-scale-cliff-summary.md`
- `docs/plan/perf-runs/raw/monorepo-001-diag.json`
- `docs/plan/perf-runs/raw/monorepo-002-diag.json`
- `docs/plan/perf-runs/raw/monorepo-003-diag.json`
- `docs/plan/perf-runs/raw/monorepo-004-diag.json`
- `docs/plan/perf-runs/raw/monorepo-005-diag.json`
- `docs/plan/perf-runs/raw/monorepo-006-diag.json`

## Verification

- `jq -e '.' docs/plan/perf-runs/raw/monorepo-001-diag.json docs/plan/perf-runs/raw/monorepo-002-diag.json docs/plan/perf-runs/raw/monorepo-003-diag.json docs/plan/perf-runs/raw/monorepo-004-diag.json docs/plan/perf-runs/raw/monorepo-005-diag.json docs/plan/perf-runs/raw/monorepo-006-diag.json`
