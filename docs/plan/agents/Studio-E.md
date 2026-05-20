# Agent Goal: Studio-E

AgentName: Studio-E
Computer: Studio
Session: E
GitHub label: `agent:Studio-E`

## Mission

Keep LSP/WASM work useful but low-bandwidth while project-corpus correctness
remains the top-line gate. Focus on smoke gates, hover/JSDoc correctness, and
test shape coverage that does not steal checker/solver bandwidth.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh Studio-E
scripts/agents/disk-preflight.sh Studio-E
scripts/agents/list-owned-work.sh Studio-E
```

## Current Assignment

- Initial priority: land, close, or clearly hand off existing PRs in this lane
  before claiming issue backlog.
- Issue context: `#8759`, `#8528`, `#8529`, `#8527`, `#8270`.
- Related PRs to inspect: `#9246`, `#9243`, `#9161`, `#8615`.
- Track: LSP roadmap companion plus roadmap Track 9 consumer boundaries.
- Next concrete step: inspect whether `#9243` and `#9161` are still the right
  active LSP smoke/hover branches. Advance one small test or close the stale
  duplicate.

## Existing Work To Inspect First

- `#9243` adds protocol smoke gate.
- `#9161` resolves JSDoc `{@link}` references in hover.
- `#9246` and `#8615` both mention fourslash shape-variant generation.

## Non-Overlap Rules

- Do not start broad LSP architecture rewrites while project rows are red.
- LSP consumes checker/solver/project outputs; it does not own type algorithms.
- WASM compatibility changes must avoid filesystem assumptions or guard them
  explicitly.

## Verification

- Prefer focused LSP tests or smoke scripts.
- Do not run full fourslash locally.
- Let ready-for-review CI run heavy gates.
