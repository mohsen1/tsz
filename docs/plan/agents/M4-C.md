# Agent Goal: M4-C

AgentName: M4-C
Computer: M4
Session: C
GitHub label: `agent:M4-C`

## Mission

Fix generic inference, contextual typing, overload inference, constructor
inference, and instantiation-session bugs as bounded solver-owned
transactions.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh M4-C
scripts/agents/disk-preflight.sh M4-C
scripts/agents/list-owned-work.sh M4-C
```

## Current Assignment

- Primary gate: all bugs fixed for inference/session behavior and Kysely-style
  contextual generic project blockers.
- Bug families: literal widening and `satisfies`, JSDoc context,
  Array/reduce/callback contextual typing, overload and constructor inference,
  repeated generic calls, type-parameter identity, stale substitutions, and
  contradictory `T`-to-`T` results.
- Architecture cleanup metric: inference session state must be transactional;
  cache keys must include substitution environment, request context,
  compatibility mode, and `this`/flow inputs that change answers.
- First live command: inspect owned PRs, then search open issues for
  `inference`, `contextual`, `satisfies`, `overload`, `constructor`, `Kysely`,
  and `JSDoc`.
- Next concrete step: group bugs by inference mode and take one bounded
  session/cache invariant at a time.

## Existing Work To Inspect First

- Recent inference and contextual typing PRs.
- Kysely project-row reductions.
- `docs/architecture/INSTANTIATION_CACHE.md`.
- M4-B relation/cache work when the failure only appears during relation.

## Non-Overlap Rules

- Treat `T` not assignable to `T` style results as cache/session bugs until
  proven otherwise.
- Do not let inference state leak between repeated generic calls.
- If the fix is relation-policy keying, coordinate with M4-B.
- If the fix is mapped/conditional evaluation, coordinate with M4-A.

## Verification

- Add tests that cover reordered declarations, repeated calls, or cache-off
  behavior when state leakage is possible.
- Prefer project-row reductions over full project runs.
- Do not run full benchmark or conformance suites locally.
