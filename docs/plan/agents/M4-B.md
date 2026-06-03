# Agent Goal: M4-B

AgentName: M4-B
Computer: M4
Session: B
GitHub label: `agent:M4-B`

## Mission

Own solver relation policy, inference/session state, variance, stable identity,
and cache contracts. Relation answers should be stable, explainable, reusable
by checker diagnostics, and safe for performance work.

This goal is not complete when a branch exists. Keep going until the scoped
change lands in `main`, then pick the next M4-B release-gate item.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh M4-B
scripts/agents/disk-preflight.sh M4-B
scripts/agents/list-owned-work.sh M4-B
```

## Current Assignment

- Primary gates: all tests pass, relation/inference/identity bugs are fixed,
  cache correctness is preserved, and project-row blockers tied to relation or
  inference move toward green.
- Bug or metric families: function parameter variance, method bivariance,
  class/static/instance compatibility, readonly/mutable array relation,
  callable interface assignment, contextual generic inference, overloads,
  constructor instantiation, stable symbol/module identity, `any` propagation,
  and relation/evaluation cache keys.
- Architecture cleanup metric: relation policy flags, inference transaction
  state, stable identity keys, and cache invalidation/reset boundaries should
  become explicit.
- First live command: inspect owned PRs, then search open issues for
  `relation`, `variance`, `assignable`, `inference`, `contextual`,
  `overload`, `readonly`, `DefId`, `module`, `TS2322`, `TS2345`, and `TS2416`.
- Next concrete step: pick one policy/session/cache invariant and prove
  cache-enabled/cache-disabled or order-independent behavior with targeted
  tests.

## Existing Work To Inspect First

- Live `agent:M4-B` PRs and recent merged relation, inference, identity, or
  cache PRs.
- `docs/architecture/RELATION_REQUEST.md`.
- `docs/architecture/INSTANTIATION_CACHE.md`.
- `docs/architecture/WELL_KNOWN_NAME_REFERENCES.md`.
- M1-B checker relation-routing work that may depend on this lane.

## Non-Overlap Rules

- Cache keys must include every semantic mode that can change relation,
  inference, or identity answers.
- Do not combine broad performance pre-sizing with semantic policy changes.
- If a checker call site needs only routing, hand off to M1-B.
- If advanced evaluation is the actual failure, coordinate with M4-A.
- If the fix requires multi-module cache/identity redesign, stack with M4-Opus.

## Verification

- Prefer targeted solver tests that compare cache-enabled and cache-disabled
  behavior where available.
- Add repeated-call/order-independence tests when touching inference sessions,
  globals, aliases, or relation/evaluation caches.
- Use `cargo nextest run`, not `cargo test`.
- Run architecture guards when boundary or policy construction moves.
