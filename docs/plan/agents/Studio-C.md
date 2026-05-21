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

- Initial priority: land, close, or clearly hand off existing PRs in this lane
  before claiming issue backlog.
- Issue context: `#8755`, `#8754`, `#8752`, `#8751`, `#8750`, `#8737`,
  `#8734`, `#8731`, `#8516`, `#8515`, `#8511`, `#8510`, `#8509`, `#8507`,
  `#8506`.
- Related PRs already drained: `#9287`, `#9308`, `#9303`, `#9299`, `#9111`,
  `#9579`, `#9625`.
- Track: roadmap Track 9.
- Next concrete step: pick a low-overlap JS emit family and reduce one baseline
  class through a transform-layer fix. Avoid active draft overlap in
  async/generator, module/import/export, class/decorator, and DTS lanes.
- Fresh probes from 2026-05-21:
  - `optionalChainingInLoop`, `newTarget`, and `tslib` are green on current
    `origin/main` despite stale checked-in snapshot entries.
  - `reserved` is still 48/51 JS passing, but the remaining cases are
    parser-recovery-heavy (`reservedNamesInAliases`, `reservedWords2`,
    `reservedWords3`).
  - `unicodeEscapesInNames02(target=es5)` and
    `jsxNamespacePrefixInName*` remain parser-recovery-heavy; only take them
    with focused parser diagnostics coverage.

## Existing Work To Inspect First

- No open Studio-C PRs at last refresh. Confirm with
  `scripts/agents/list-owned-work.sh Studio-C` each cycle.

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
