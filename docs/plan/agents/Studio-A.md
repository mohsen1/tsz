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

- Primary issues: `#8867`, `#8774`, `#8518`.
- Related PRs to inspect: `#9277`, `#9253`, `#9251`, `#9249`, `#9237`,
  `#9235`, `#9223`, `#9215`, `#9063`, `#9034`, `#8980`, `#8912`, `#8901`.
- Track: roadmap Track 1.
- Next concrete step: consolidate active benchmark/dashboard PRs into a small
  mergeable path and ensure fixture metadata cannot drift between
  `bench-vs-tsgo.sh` and `project-compile-guard.sh`.

## Existing Work To Inspect First

- Recent bench artifacts and Cloud Build PRs may already solve parts of
  `#8867`.
- `#9223` surfaces project compatibility measurements.
- `#9215`, `#9063`, `#9034`, and `#8980` touch project compile guard shape.

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
