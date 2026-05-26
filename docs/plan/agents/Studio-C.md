# Agent Goal: Studio-C

AgentName: Studio-C
Computer: Studio
Session: C
GitHub label: `agent:Studio-C`

## Mission

Recover JavaScript emit parity to `13,530 / 13,530` by named transform
families while keeping emit free of semantic type validation.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh Studio-C
scripts/agents/disk-preflight.sh Studio-C
scripts/agents/list-owned-work.sh Studio-C
python3 scripts/emit/query-emit.py --families
```

## Current Assignment

- Primary gate: JavaScript emit 100%.
- Bug families from the checked-in artifact: class/private/accessor/decorator
  lowering, module/import/export emit, async/await/generator lowering,
  resource-management lowering, destructuring/spread/rest emit, JSX/react emit,
  parser/recovery emit, literal/template emit, and final-mile `other`.
- Architecture cleanup metric: move complex transforms toward typed
  `EmitPlan`/IR facts; reduce ambient `Printer` state, source-text recovery,
  and output-surgery pressure.
- First live command: run `python3 scripts/emit/query-emit.py --families`, then
  choose the largest unowned JS family with no active PR.
- Next concrete step: reduce one baseline family through a transform-layer fix
  and exact/baseline-style targeted verification.

## Existing Work To Inspect First

- Open `agent:Studio-C` PRs.
- Issues `#8506`, `#8750`, `#8507`, `#8510`, `#8509`, `#8512`, `#9330`,
  `#9331`, `#9334`, `#9335`, `#9336`, `#9337`, `#9338`, and `#9339`.
- `docs/architecture/EMIT_ARCHITECTURE.md`.
- Recent merged emit PRs for the same transform family.

## Non-Overlap Rules

- Emit must not import checker internals or perform semantic validation.
- Parser recovery facts are acceptable inputs; source-substring guessing is
  migration debt, not precedent.
- Do not bundle JS emit with DTS fixes unless the baseline family genuinely
  shares the same output-layer rule.
- Do not patch already-emitted text for semantic behavior.

## Verification

- Use narrow emit filters through `scripts/emit/run.sh` only for the family in
  scope.
- Use `python3 scripts/emit/query-emit.py --filter <family>` for offline
  artifact orientation.
- Do not run the full emit suite locally.
- Prefer exact output/baseline-style checks over fragment smoke tests.
