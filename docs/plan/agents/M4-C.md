# Agent Goal: M4-C

AgentName: M4-C
Computer: M4
Session: C
GitHub label: `agent:M4-C`

## Mission

Fix generic inference, contextual typing, constructor inference, and
instantiation-session bugs as bounded solver-owned transactions.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh M4-C
scripts/agents/disk-preflight.sh M4-C
scripts/agents/list-owned-work.sh M4-C
```

## Current Assignment

- Initial priority: land, close, or clearly hand off existing PRs in this lane
  before claiming issue backlog.
- Issue context: `#8773`, `#8711`, `#8707`, `#8703`, `#6407`.
- Related PRs to inspect: `#9257`, `#9224`, `#9200`, `#9103`, `#9002`,
  `#8901`, `#8471`, `#8466`.
- Track: roadmap Track 3.
- Next concrete step: drain ready PR `#9103` if still open, then focus on the
  Kysely/Zod contextual generic constructor path with a reduced targeted test.

## Existing Work To Inspect First

- `#8901` targets Kysely generic relation blockers.
- `#9200` targets Zod contextual this factory returns.
- `#8471` and `#8466` stage typed solver request/result boundaries.

## Non-Overlap Rules

- Treat `T` not assignable to `T` style results as cache/session bugs until
  proven otherwise.
- Do not let inference state leak between repeated generic calls.
- If the fix is relation-policy keying, coordinate with M4-B.

## Verification

- Add tests that cover reordered declarations or repeated calls when state
  leakage is possible.
- Prefer project-row reductions over full project runs.
- Do not run full benchmark or conformance suites locally.
