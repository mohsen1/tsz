# Agent Goal: M4-B

AgentName: M4-B
Computer: M4
Session: B
GitHub label: `agent:M4-B`

## Mission

Consolidate relation policy and cache-key protocols so relation answers are
stable, explainable, and shared by checker diagnostics.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh M4-B
scripts/agents/disk-preflight.sh M4-B
scripts/agents/list-owned-work.sh M4-B
```

## Current Assignment

- Initial priority: land, close, or clearly hand off existing PRs in this lane
  before claiming issue backlog.
- Issue context: `#8207`, `#8203`.
- Active stack: `#9265`, `#9268`, `#9281`, `#9289`.
- Ready performance PRs to drain or coordinate around: `#9297`, `#9298`.
- Track: roadmap Tracks 3, 4, and 10.
- Next concrete step: inspect the relation-policy stack and either advance the
  next child PR or collapse stale children into a smaller mergeable slice.

## Existing Work To Inspect First

- `#9265` is the root relation engine flag routing PR.
- `#9268`, `#9281`, and `#9289` are stacked on top of relation policy changes.
- M1-B depends on this lane for checker relation gateway cleanup.

## Non-Overlap Rules

- Cache keys must include every semantic mode that can change relation answers.
- Do not combine broad performance pre-sizing with semantic policy changes.
- If a checker call site needs only routing, hand off to M1-B.

## Verification

- Prefer targeted solver tests that compare cache-enabled and cache-disabled
  behavior where available.
- Record behavior unchanged for pure refactors.
- Use `cargo nextest run`, not `cargo test`.
