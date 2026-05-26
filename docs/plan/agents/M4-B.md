# Agent Goal: M4-B

AgentName: M4-B
Computer: M4
Session: B
GitHub label: `agent:M4-B`

## Mission

Consolidate solver relation policy, variance, compatibility exceptions, and
cache-key protocols so relation answers are stable, explainable, and shared by
checker diagnostics.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh M4-B
scripts/agents/disk-preflight.sh M4-B
scripts/agents/list-owned-work.sh M4-B
```

## Current Assignment

- Primary gate: all bugs fixed for relation, variance, call/class
  compatibility, and relation-cache correctness.
- Bug families: function parameter variance, method bivariance exceptions,
  class/static/instance compatibility, readonly/mutable array relation,
  callable interface assignment, accessor compatibility, excess/freshness,
  weak types, `any` propagation, and relation fuel/complexity.
- Architecture cleanup metric: relation policy flags and cache keys must be
  explicit; legacy flag protocols and direct policy construction outside query
  boundaries should shrink.
- First live command: inspect owned PRs, then search open issues for
  `relation`, `variance`, `assignable`, `readonly`, `TS2322`, `TS2345`,
  `TS2416`, and solver `tech-debt`.
- Next concrete step: pick one policy/cache invariant and prove
  cache-enabled/cache-disabled agreement with targeted tests.

## Existing Work To Inspect First

- Issues `#8207` and `#8203` for solver architecture boundary debt.
- `docs/architecture/RELATION_REQUEST.md`,
  `docs/architecture/INSTANTIATION_CACHE.md`, and relation policy modules.
- M1-B checker relation-routing work that may depend on this lane.

## Non-Overlap Rules

- Cache keys must include every semantic mode that can change relation answers.
- Do not combine broad performance pre-sizing with semantic policy changes.
- If a checker call site needs only routing, hand off to M1-B.
- If an evaluation or inference bug only appears through relation, coordinate
  with M4-A or M4-C before changing policy.

## Verification

- Prefer targeted solver tests that compare cache-enabled and cache-disabled
  behavior where available.
- Record behavior unchanged for pure refactors.
- Use `cargo nextest run`, not `cargo test`.
- Run architecture guards when boundary or policy construction moves.
