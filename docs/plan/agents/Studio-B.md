# Agent Goal: Studio-B

AgentName: Studio-B
Computer: Studio
Session: B
GitHub label: `agent:Studio-B`

## Mission

Own performance and residency until every eligible green timed benchmark row is
at least `2x` faster than `tsgo`, without bypassing semantic parity.

This goal is not complete when a branch exists. Keep going until the scoped
change lands in `main`, then pick the next Studio-B release-gate item.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh Studio-B
scripts/agents/disk-preflight.sh Studio-B
scripts/agents/list-owned-work.sh Studio-B
```

## Current Assignment

- Primary gate: zero `two_x_target.target_gaps` for eligible green timed rows.
- Bug or metric families: tsgo-winning rows, ts-toolbelt/type-fest/large-repo
  residency, cross-arena delegation overhead, file-session reuse overhead,
  runtime/OOM/timeout blockers, and cache/counter-driven hot paths.
- Architecture cleanup metric: cache keys, request scopes, stable identity, and
  bounded residency must become more explicit; no fixture-name fast paths.
- First live command: inspect owned PRs, then locate the latest
  `bench-vs-tsgo*.json` and `*.tsgo-winners.json` artifacts before making a
  timing claim.
- Next concrete step: take one green-row target gap or runtime/residency red
  row, record before artifact fields, and choose the smallest semantic-safe
  cache/residency invariant to improve.

## Existing Work To Inspect First

- Live `agent:Studio-B` PRs.
- `docs/plan/PERFORMANCE_PLAN.md`.
- Latest `scripts/bench/tsgo-winner-report.mjs` output and project-row
  compatibility artifact.
- Recent semantic/cache PRs from M4-B and M4-Opus before repeating cache work.

## Non-Overlap Rules

- Broad speed tuning waits until rows are green, unless the first blocker is
  runtime, OOM, timeout, or residency.
- Performance PRs state benchmark, before/after command, diagnostic status,
  measurement mode, RSS when relevant, and semantic invariant.
- Do not use fixture names as fast-path keys.
- If a row is red because of diagnostics or semantic failure, hand it to the
  owning M1/M4 lane.

## Verification

- Use `scripts/bench/perf-hotspots.sh --quick` or narrow project filters.
- Use `scripts/bench/tsgo-winner-report.mjs <bench-results.json> <output.json>`
  when a fresh benchmark artifact exists.
- Wrap heavy runs with `scripts/safe-run.sh`.
- Do not run full conformance, full emit, full fourslash, or broad benchmarks
  locally.
