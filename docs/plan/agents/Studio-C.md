# Agent Goal: Studio-C

AgentName: Studio-C
Computer: Studio
Session: C
GitHub label: `agent:Studio-C`

## Mission

Recover JavaScript emit parity by named transform families while keeping emit
free of semantic type validation.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh Studio-C
scripts/agents/disk-preflight.sh Studio-C
scripts/agents/list-owned-work.sh Studio-C
```

## Current Assignment

- Primary issues: `#8755`, `#8754`, `#8752`, `#8751`, `#8750`, `#8737`,
  `#8734`, `#8731`, `#8516`, `#8515`, `#8511`, `#8510`, `#8509`, `#8507`,
  `#8506`.
- Related PRs to inspect: `#9287`, `#9308`, `#9303`, `#9299`, `#9111`.
- Track: roadmap Track 9.
- Next concrete step: drain ready emit PR `#9287` if still open, then pick one
  JS emit family and reduce one baseline class through a transform-layer fix.

## Existing Work To Inspect First

- `#9308` computed-key temps outside ES5 class IIFEs.
- `#9303` native destructuring in non-ES5 parameter prologues.
- `#9299` concise arrow comment placement.
- `#9111` parser recovery for trailing decimal emit behavior.

## Non-Overlap Rules

- Emit must not import checker internals or perform semantic validation.
- Parser recovery facts are acceptable inputs; source-substring guessing is
  migration debt, not precedent.
- Do not bundle JS emit with DTS fixes unless the baseline family genuinely
  shares the same output-layer rule.

## Verification

- Use narrow emit filters through `scripts/emit/run.sh` only for the family in
  scope.
- Do not run the full emit suite locally.
- Prefer exact output/baseline-style checks over fragment smoke tests.
