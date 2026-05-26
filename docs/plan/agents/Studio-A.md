# Agent Goal: Studio-A

AgentName: Studio-A
Computer: Studio
Session: A
GitHub label: `agent:Studio-A`

## Mission

Make release metric truth authoritative. This lane owns project-corpus
artifact truth and keeps conformance strictness, emit counts, bug-family
ownership, and performance target artifacts coherent before other lanes make
claims from them.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh Studio-A
scripts/agents/disk-preflight.sh Studio-A
scripts/agents/list-owned-work.sh Studio-A
python3 scripts/conformance/query-conformance.py --dashboard
python3 scripts/emit/query-emit.py --families
node scripts/bench/project-row-summary.mjs --markdown
```

## Current Assignment

- Primary gate: project corpus and public release metrics.
- Bug or metric families: project-row green/yellow/red status, first blocker
  family, diagnostic deltas, emit/DTS snapshot counts, accepted-regression
  count, issue-cluster ownership, benchmark artifact freshness, and project-row
  metadata consistency.
- Architecture cleanup metric: row definitions should stay centralized in
  `scripts/bench/project-rows.mjs`; dashboard and guard surfaces should not
  drift.
- First live command: run the cheap dashboard commands above and record which
  artifact is current enough to cite.
- Next concrete step: if a metric is stale or contradictory, fix the reporting
  surface or route the discrepancy to the owning lane before anyone optimizes
  against it.

## Existing Work To Inspect First

- `scripts/bench/project-rows.mjs`, `scripts/bench/project-rows.md`, and
  `scripts/bench/validate-project-metadata.mjs`.
- Latest benchmark, compile-guard, conformance, and emit artifacts.
- Open issues labelled `bench`, `performance`, `conformance`, `emit`, and
  `accepted-regression`.
- Recent website/README metric updates.

## Non-Overlap Rules

- Do not present a faster red row as a speed win without naming the correctness
  blocker.
- Do not rerun broad benchmarks locally. Use narrow filters only when they
  answer one debugging question.
- Metric docs should cite artifact fields or CI URLs, not stale copied counts.
- Root-cause reductions belong in owning-crate tests once understood.

## Verification

- Prefer script tests under `scripts/bench`, `scripts/ci`, `scripts/emit`, or
  `scripts/conformance`.
- Use `node scripts/bench/validate-project-metadata.mjs` for row metadata.
- Wrap long project checks with `scripts/safe-run.sh`.
