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
- `#9650` was ready earlier in the 2026-05-21 audit window, then moved back to
  draft after CI/body blockers. Inspect it before reviving or replacing it.
- Active relation-policy stack: `#9265`, `#9268`, `#9281`, `#9289`.
- Current draft/new-issue cluster: `#9807`, `#9803`, `#9800`, `#9798`,
  `#9650`, `#9230`, `#8207`, and `#8203`.
- Track: roadmap Tracks 3, 4, and 10.
- Next concrete step: resolve why `#9650` is draft again, then collapse or
  advance the relation-policy stack root-first. Do not start another
  policy/cache branch until the stack has one clear next merge.

## Existing Work To Inspect First

- `#9265` is the root relation engine flag routing PR.
- `#9268`, `#9281`, and `#9289` are stacked on top of relation policy changes.
- M1-B depends on this lane for checker relation gateway cleanup.
- `#9803` is titled `[WIP]`; keep it WIP until the owner leaves a signed
  status comment and removes the title prefix.

## Non-Overlap Rules

- Cache keys must include every semantic mode that can change relation answers.
- Do not combine broad performance pre-sizing with semantic policy changes.
- If a checker call site needs only routing, hand off to M1-B.

## Verification

- Prefer targeted solver tests that compare cache-enabled and cache-disabled
  behavior where available.
- Record behavior unchanged for pure refactors.
- Use `cargo nextest run`, not `cargo test`.
