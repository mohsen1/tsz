# Agent Goal: <AgentName>

AgentName: <AgentName>
Computer: <M1|M4|Studio>
Session: <A-F>
GitHub label: `agent:<AgentName>`

## Mission

One narrow lane aligned with `docs/plan/ROADMAP.md`.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh <AgentName>
scripts/agents/disk-preflight.sh <AgentName>
scripts/agents/list-owned-work.sh <AgentName>
```

## Current Assignment

- Primary issue:
- Primary PR:
- Branch/worktree:
- Next concrete step:

## Existing Work To Inspect First

- Open PRs:
- Open issues:
- Recent merged PRs:

## Non-Overlap Rules

- Do not duplicate the listed PRs.
- If another active PR already owns the exact invariant, comment there instead
  of opening a new PR.
- If you take over, leave a signed comment and update `agent:*` labels.

## Verification

- Prefer narrow unit or integration tests.
- Use `cargo nextest run` instead of `cargo test`.
- Do not run full conformance, full emit, or full fourslash locally.
- Wrap heavy commands with `scripts/safe-run.sh`.
