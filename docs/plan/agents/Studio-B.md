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
- Issue context: `#8869`, `#8868`, `#8870`, `#8356`, `#7574`, `#7378`,
  `#7531`, `#8857`, `#8858`.
- Related PRs to inspect: `#9290`, `#9291`, `#9292`, `#9293`, `#9294`,
  `#9295`, `#9286`, `#9275`, `#9273`, `#9267`, `#9266`, `#9264`, `#9261`,
  `#8913`, `#9208`.
- Track: roadmap Tracks 2 and 10.
- Next concrete step: separate mergeable pre-sizing/measurement PRs from true
  semantic/runtime blockers, then make sure ready performance PRs do not sit
  behind stale WIP state.

## Existing Work To Inspect First

- Many `studiofast/*capacity` PRs are open. Do not open another capacity PR
  until the ready/draft queue is triaged.
- `#8913` shares application/evaluation caches cross-file.
- `#9208` direct-lowers DOM lib delegation fallbacks.

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
