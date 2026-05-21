# Agent Goal: Studio-B

AgentName: Studio-B
Computer: Studio
Session: B
GitHub label: `agent:Studio-B`

## Mission

Own project-row performance and residency blockers, especially ts-toolbelt and
type-fest scale issues, without bypassing semantic parity.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh Studio-B
scripts/agents/disk-preflight.sh Studio-B
scripts/agents/list-owned-work.sh Studio-B
```

## Current Assignment

- Initial priority: land, close, or clearly hand off existing PRs in this lane
  before claiming issue backlog.
- `#9819` and `#9829` merged during the 2026-05-21 audit window and now serve
  as the three-point recursive-utility hotspot guard for this lane. `#8870` is
  closed.
- Issue context: `#8868`, `#8858`, `#8356`, `#7574`, `#7378`, `#7531`,
  `#8857`, and the closed-but-informative `#8869`/`#8870`.
- Related recent merges to inspect for benchmark-readiness guardrails:
  `#9813`, `#9794`, `#9789`, `#9626`, `#9587`, and `#9581`.
- Track: roadmap Tracks 2 and 10.
- Next concrete step: use the `#9819`/`#9829` guard, `#8868`, and `#7378` to
  decide whether the next slice is semantic recursion work, measurement
  readiness, or residency attribution.

## Existing Work To Inspect First

- Recent Studio-A merges tightened readiness and duplicate winner-row checks;
  do not bypass those artifacts when proving a performance win.
- `#9819`/`#9829` add recursive utility alias hotspot rows; keep them as
  benchmark guards rather than broad speed-tuning branches.
- If a benchmark row is red because of diagnostics, hand it to the owning
  semantic lane before measuring runtime.

## Non-Overlap Rules

- Broad speed tuning waits until rows are green, unless the first blocker is
  runtime, OOM, timeout, or residency.
- Performance PRs state benchmark, before/after command, diagnostic status,
  RSS when relevant, and semantic invariant.
- Do not use fixture names as fast-path keys.

## Verification

- Use `scripts/bench/perf-hotspots.sh --quick` or narrow project filters.
- Wrap heavy runs with `scripts/safe-run.sh`.
- Do not run full conformance, full emit, or full fourslash locally.
