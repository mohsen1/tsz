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

- Primary PR to land/close/handoff:
- Assigned draft PRs to complete before new issue work:
- Issue context:
- Branch/worktree:
- Next concrete step:

## Existing Work To Inspect First

- Open PRs:
- Open issues:
- Recent merged PRs:

## Non-Overlap Rules

- Do not duplicate the listed PRs.
- Do not start a new branch while assigned draft PRs are missing a ready,
  merged, closed-with-evidence, or signed-handoff state.
- If another active PR already owns the exact invariant, comment there instead
  of opening a new PR.
- If you take over, leave a signed comment and update `agent:*` labels.

## Verification

- Prefer narrow unit or integration tests.
- Use `cargo nextest run` instead of `cargo test`.
- Do not run full conformance, full emit, or full fourslash locally.
- Wrap heavy commands with `scripts/safe-run.sh`.
