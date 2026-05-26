# Agent Goal: Studio-D

AgentName: Studio-D
Computer: Studio
Session: D
GitHub label: `agent:Studio-D`

## Mission

Recover declaration emit parity to `1,669 / 1,669` while moving toward
declaration/public-API summary boundaries instead of late semantic discovery
during printing.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh Studio-D
scripts/agents/disk-preflight.sh Studio-D
scripts/agents/list-owned-work.sh Studio-D
python3 scripts/emit/query-emit.py --dts-failures --top 25
```

## Current Assignment

- Primary gate: declaration emit 100%.
- Bug families: generic/type-display declarations, import/export/nameability,
  recursive unique type-parameter renaming, module/declaration merging,
  class/private/accessor declarations, unique-symbol declarations, and
  final-mile DTS `other`.
- Architecture cleanup metric: DTS fixes should consume a
  `DeclarationSummary`/`PublicApiSummary` fact or add one upstream; direct
  solver reach-through and fresh type evaluation during printing should trend
  down.
- First live command: run the DTS query command above and choose one unowned
  declaration family.
- Next concrete step: document whether the family has an existing summary fact
  or requires a new one, then fix through that boundary.

## Existing Work To Inspect First

- Issues `#9332`, `#8275`, `#8683`, `#8720`, and DTS-labelled bug issues.
- `docs/architecture/EMIT_ARCHITECTURE.md` declaration summary section.
- Recent DTS JSDoc, import/nameability, and type-display PRs.
- M4-D when a DTS gap is actually stable identity or module identity.

## Non-Overlap Rules

- DTS fixes should not add fresh type evaluation during printing.
- Do not use display string fragments as proof of parity.
- If a semantic fact is missing, add or propose a declaration summary or query
  boundary rather than reaching through solver internals.
- Coordinate with Studio-E for JSDoc/JavaScript declaration-specific gaps.

## Verification

- Use narrow DTS/emit filters.
- Do not run the full emit suite locally.
- State why checker diagnostics are unaffected.
