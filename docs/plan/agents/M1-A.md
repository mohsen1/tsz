# Agent Goal: M1-A

AgentName: M1-A
Computer: M1
Session: A
GitHub label: `agent:M1-A`

## Mission

Keep the launch legible. This lane owns live ownership hygiene, release-gate
scoreboard coordination, duplicate-work prevention, and bug-triage flow after
the old PR runway has been closed or clearly handed off.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh M1-A
scripts/agents/disk-preflight.sh M1-A
scripts/agents/list-owned-work.sh M1-A
scripts/agents/ensure-agent-labels.sh --audit
node scripts/ci/pr-ownership-report.mjs
```

## Current Assignment

- Primary gate: coordination health for conformance strictness, emit 100%, bug
  closure, green project rows, and `2x` perf target work.
- Live PR rule: if any `agent:M1-A` PRs are open, land, close, or hand them off
  before filing new coordination work.
- Bug-triage contract: every open `bug`, `false-positive`, `false-negative`,
  `accepted-regression`, and issue-level `WIP` should have a lane owner, a
  duplicate/superseded/upstream/non-release explanation, or an active draft PR.
- Architecture cleanup metric: ownership report mismatches, noncanonical
  labels, duplicate active invariants, stale WIP markers, and claim-doc drift
  should trend to zero.
- Next concrete step: produce a live issue/PR handoff summary when the launch
  starts, then route clusters to the owning lanes instead of taking over their
  implementation.

## Existing Work To Inspect First

- `scripts/agents/list-owned-work.sh --all`.
- `node scripts/ci/pr-ownership-report.mjs`.
- `scripts/agents/ensure-agent-labels.sh --audit`.
- Recent merged PRs touching `docs/plan/agents`, `scripts/ci/pr-ownership-report.mjs`,
  and label/WIP tooling.
- Open issues with `bug`, `false-positive`, `false-negative`,
  `accepted-regression`, `urgent`, `help wanted`, or `WIP`.

## Non-Overlap Rules

- Do not take over implementation lanes unless the current owner asks or a
  stale branch needs a signed handoff.
- When changing WIP state, leave a signed comment with reason, blocker/current
  work, and next owner/action.
- Do not create new claim documents under `docs/plan/claims`.
- Prefer routing and consolidation over opening new coordination-only PRs.

## Verification

- Use `scripts/ci/pr-ownership-report.mjs` for PR topology.
- Use `scripts/agents/ensure-agent-labels.sh --audit` for label hygiene.
- Use `scripts/ci/check-wip-state-comments.mjs` when changing WIP state.
- No compiler suite is needed for metadata-only cleanup.
