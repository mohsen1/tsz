# Agent Goal: M4-A

AgentName: M4-A
Computer: M4
Session: A
GitHub label: `agent:M4-A`

## Mission

Own recursive conditional, mapped, and alias identity evaluation bugs that block
project rows and conformance parity. Keep evaluation solver-owned and avoid
checker symptom patches.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh M4-A
scripts/agents/disk-preflight.sh M4-A
scripts/agents/list-owned-work.sh M4-A
```

## Current Assignment

- Initial priority: land, close, or clearly hand off existing PRs in this lane
  before claiming issue backlog.
- Current ready PR to drain: `#9804`.
- Current draft cluster to reconcile before new branches: `#9816`, `#9776`,
  `#9656`, `#9647`, `#9644`, `#9640`, `#9624`, `#9515`, `#9205`, `#9820`,
  and `#9586`.
- Issue context: `#9784`, `#9777`, `#9772`, `#9767`, `#9759`, `#9749`,
  `#9743`, `#9740`, `#9305`, `#8778`, `#8726`, `#8702`, `#8687`, `#8423`,
  and `#7648`.
- Track: roadmap Track 2.
- Next concrete step: get `#9804` through current CI if it is still ready, then
  cluster the recursive conditional, tuple/rest, template-literal infer, and
  mapped-key issues into one owner per structural rule.

## Existing Work To Inspect First

- Recent ready work moved from the old `#9304` queue to `#9804`.
- `#9515`, `#9820`, and `#9642` overlap template-literal/keyof/conformance
  work; avoid reopening the same invariant under another generated lane.
- Emit/DTS-looking M4-A drafts (`#9776`, `#9656`, `#9647`, `#9644`, `#9640`,
  `#9624`, `#9205`) should be handed to Studio-C/Studio-D if they stop being
  solver/evaluation blockers.

## Non-Overlap Rules

- Do not add test-name, alias-name, or display-string special cases.
- Do not erase deferred conditionals to `any` or `error` to silence one
  diagnostic.
- If the issue is a cache-key/policy bug, coordinate with M4-B.

## Verification

- Add solver or checker tests with renamed type parameters and alias/wrapper
  variants.
- Use narrow `cargo nextest run` filters.
- Do not run full conformance locally.
