# Agent Goal: Studio-D

AgentName: Studio-D
Computer: Studio
Session: D
GitHub label: `agent:Studio-D`

## Mission

Recover declaration emit parity while moving toward declaration/public-API
summary boundaries instead of late semantic discovery during printing.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh Studio-D
scripts/agents/disk-preflight.sh Studio-D
scripts/agents/list-owned-work.sh Studio-D
```

## Current Assignment

- Primary issues: `#8747`, `#8746`, `#8745`, `#8743`, `#8720`, `#8683`,
  `#8682`, `#8522`, `#8275`, `#8276`.
- Related PRs to inspect: `#9313`, `#9312`, `#9310`, `#9205`, `#9198`,
  `#9151`, `#9096`, `#9028`, `#8958`, `#8940`, `#8574`.
- Track: roadmap Track 9.
- Next concrete step: drain ready DTS PR `#9313` if still open, then pick one
  DTS family and document whether the fix consumes an existing summary fact or
  requires a new declaration-summary input.

## Existing Work To Inspect First

- `#9312`, `#9310`, `#9028`, and `#9151` overlap unreachable logical
  initializer or parenthesized type handling.
- `#9205` and `#9198` expand alias display in DTS.
- `#9096` handles named tuple labels.

## Non-Overlap Rules

- DTS fixes should not add fresh type evaluation during printing.
- Do not use display string fragments as proof of parity.
- If a semantic fact is missing, propose a declaration summary or compiler
  service boundary rather than reaching through solver internals.

## Verification

- Use narrow DTS/emit filters.
- Do not run the full emit suite locally.
- State why checker diagnostics are unaffected.
