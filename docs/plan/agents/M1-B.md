# Agent Goal: M1-B

AgentName: M1-B
Computer: M1
Session: B
GitHub label: `agent:M1-B`

## Mission

Own checker orchestration: relation diagnostic routing, flow/narrowing handoff,
and query-boundary cleanup. Move checker paths toward request-shaped boundary
APIs and away from raw solver reach-through.

This goal is not complete when a branch exists. Keep going until the scoped
change lands in `main`, then pick the next M1-B release-gate item.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh M1-B
scripts/agents/disk-preflight.sh M1-B
scripts/agents/list-owned-work.sh M1-B
scripts/arch/check-checker-boundaries.sh
```

## Current Assignment

- Primary gates: all tests pass, bugs in checker relation/flow orchestration
  are fixed, and architecture guard debt trends down.
- Bug or metric families: assignment/argument/override relation routing,
  flow-sensitive narrowing handoff, excess/freshness orchestration, query
  boundary quarantine, checker-local solver reach-through, and diagnostic
  reason propagation.
- Architecture cleanup metric: direct checker relation call sites, broad
  query-boundary barrels, and checker-owned semantic traversal should trend
  down.
- First live command: inspect owned PRs, then run the checker boundary guard and
  identify one failing or high-debt path.
- Next concrete step: route one checker call path through an existing or narrow
  new boundary helper, with behavior unchanged unless a focused bug test proves
  the intended structural rule.

## Existing Work To Inspect First

- Live `agent:M1-B` PRs and recent merged checker relation/query-boundary PRs.
- `docs/architecture/RELATION_REQUEST.md`.
- `docs/architecture/QUERY_BOUNDARY_INVENTORY.md`.
- Issues `#8227`, `#8225`, and `#8223` for durable boundary debt context.

## Non-Overlap Rules

- New checker code should not call solver internals directly when a
  query-boundary helper can own the request.
- If the fix needs variance, relation policy, `any` propagation, inference
  sessions, or cache-key semantics, coordinate with M4-B or M4-Opus.
- If the fix only changes diagnostic wording or strictness artifacts,
  coordinate with M1-A.
- Every behavior-changing PR states the structural rule and adjacent cases.

## Verification

- Prefer targeted checker tests or narrow `cargo nextest run -p tsz_checker`.
- Run `scripts/arch/check-checker-boundaries.sh` after boundary changes.
- Run `python3 scripts/arch/arch_guard.py` when architecture guard rules or
  caps are touched.
- Do not run full conformance locally.
