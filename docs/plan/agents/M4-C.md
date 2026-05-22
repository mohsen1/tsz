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
- Current ready queue: `#9827`, `#9814`, `#9808`, and `#9799`.
- Current draft cluster to reconcile before new branches: `#9810`, `#9809`,
  `#9801`, `#9797`, `#9792`, `#9508`, `#9224`, and `#9200`.
- Issue context: `#9785`, `#9778`, `#9775`, `#9774`, `#9773`, `#9769`,
  `#9768`, `#9766`, `#9765`, `#9762`, `#9761`, `#9760`, `#9758`,
  `#9757`, `#9756`, `#9754`, `#9747`, `#9746`, `#9745`, `#8773`, `#8711`,
  `#8707`, `#8703`, and `#6407`.
- Track: roadmap Track 3.
- Next concrete step: let green auto-merge complete on the ready queue, fix the
  current failing CI if one remains, then group the new issues by inference
  mode: literal widening/`satisfies`, JSDoc context, unique-symbol/keyof, and
  nullish/operator diagnostics.

## Existing Work To Inspect First

- `#9801` is the JSDoc enforcement branch and is draft again after a failed
  conformance aggregate; do not start another `@implements` branch until that
  result is resolved or handed off.
- `#9814`, `#9799`, `#9785`, `#9773`, `#9765`, and `#9758` all touch
  literal widening and `satisfies` or const-context state.
- `#9808`, `#9810`, `#9766`, `#9755`, and `#9747` overlap unique-symbol,
  `keyof`, and element-access diagnostics.

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
