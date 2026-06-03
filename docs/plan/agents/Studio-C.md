# Agent Goal: Studio-C

AgentName: Studio-C
Computer: Studio
Session: C
GitHub label: `agent:Studio-C`

## Mission

Own JavaScript emit and declaration emit parity, output-surgery burn-down, and
emit boundary cleanup. Reach `100%` against TypeScript baselines without moving
semantic validation into emit.

This goal is not complete when a branch exists. Keep going until the scoped
change lands in `main`, then pick the next Studio-C release-gate item.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh Studio-C
scripts/agents/disk-preflight.sh Studio-C
scripts/agents/list-owned-work.sh Studio-C
python3 scripts/emit/query-emit.py --families
python3 scripts/emit/audit-output-surgery.py
```

## Current Assignment

- Primary gates: JavaScript emit `100%`, declaration emit `100%`, and output
  surgery debt ratchets down instead of expanding.
- Bug or metric families: JS transform families, DTS nameability/portability,
  declaration/public API summaries, JSDoc/JS declarations, module/import/export
  emit, class/private/accessor/decorator lowering, async/generator/resource
  lowering, JSX/react emit, parser/recovery emit, and final-mile `other`.
- Architecture cleanup metric: move complex transforms toward typed
  `EmitPlan`, declaration summary, or recovery facts; reduce ambient `Printer`
  state, source-text recovery, and output-surgery pressure.
- First live command: run the emit family and output-surgery commands above,
  then choose the largest unowned family or the highest-risk surgery entry.
- Next concrete step: reduce one baseline family through an output-layer fix
  and exact/baseline-style targeted verification.

## Existing Work To Inspect First

- Live `agent:Studio-C` PRs.
- `docs/architecture/EMIT_ARCHITECTURE.md`.
- `scripts/emit/output-surgery-allowlist.txt`.
- Recent merged JS/DTS emit PRs for the same transform family.
- Open issues labelled `emit`, `dts`, `baseline`, `jsdoc`, and `tech-debt`.

## Non-Overlap Rules

- Emit must not import checker internals or perform semantic validation.
- Parser recovery facts and declaration summaries are acceptable inputs;
  source-substring guessing is migration debt, not precedent.
- Do not bundle JS emit with DTS fixes unless the baseline family genuinely
  shares the same output-layer rule.
- If the gap requires type facts the emitter does not own, coordinate with
  Studio-Opus, M4-B, or M4-Opus.

## Verification

- Use narrow emit filters through `scripts/emit/run.sh` only for the family in
  scope.
- Use `python3 scripts/emit/query-emit.py --filter <family>` for offline
  artifact orientation when supported by the script.
- Run `python3 scripts/emit/audit-output-surgery.py` when touching output
  surgery or emit printing shortcuts.
- Do not run the full emit suite locally.
