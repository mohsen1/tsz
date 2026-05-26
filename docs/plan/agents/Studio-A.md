# Agent Goal: Studio-A

AgentName: Studio-A
Computer: Studio
Session: A
GitHub label: `agent:Studio-A`

## Mission

Make project-corpus truth authoritative. Dashboard rows must report correctness
separately from speed and name the first blocker family for red or yellow rows.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh Studio-A
scripts/agents/disk-preflight.sh Studio-A
scripts/agents/list-owned-work.sh Studio-A
```

## Current Assignment

- Initial priority: land, close, or clearly hand off existing PRs in this lane
  before claiming issue backlog.
- Issue context: `#8867`, `#8774`, `#8518`.
- Related PRs to inspect: `#9277`, `#9253`, `#9251`, `#9249`, `#9237`,
  `#9235`, `#9223`, `#9215`, `#9063`, `#9034`, `#8980`, `#8912`, `#8901`.
- Track: roadmap Track 1.
- Next concrete step: with #10035 merged and Studio-D's #9229 closed unmerged,
  Studio-A is reclaiming the unowned #8774 type-fest reduction-fixture backlog
  on branch `codex/studio-a-typefest-reductions-20260524`. Do not duplicate
  that slice while the Studio-A PR is open; after it lands, look next for
  unowned dashboard-truth gaps where a project row can misreport correctness,
  phase, first blocker family, runner identity, or fixture metadata.

## Existing Work To Inspect First

- Recent bench artifacts and Cloud Build PRs have closed `#8867`; use them as
  current assumptions instead of reopening the Cloud Build timing-shard work.
- `#9223` surfaces project compatibility measurements.
- `#9215`, `#9063`, `#9034`, and `#8980` touch project compile guard shape.
- `#9229` was the previous Studio-D draft for `#8774`, but it is now closed
  unmerged. The current type-fest reduction-fixture ownership is the Studio-A
  branch named above.

## Non-Overlap Rules

- Do not present a faster red row as a speed win without naming the correctness
  blocker.
- Do not rerun broad benchmarks locally. Use narrow filters only when they
  answer one debugging question.
- Root-cause reductions belong in owning-crate tests once understood.

## Verification

- Prefer script tests under `scripts/bench` or `scripts/ci`.
- Use fixture JSON where possible.
- Wrap long project checks with `scripts/safe-run.sh`.
