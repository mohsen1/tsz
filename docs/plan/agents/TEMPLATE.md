# Agent Goal: <AgentName>

AgentName: <AgentName>
Computer: <M1|M4|Studio>
Session: <A-F>
GitHub label: `agent:<AgentName>`

## Mission

One narrow lane aligned with `docs/plan/ROADMAP.md` and the next-launch gates:
conformance strictness, emit 100%, all bugs fixed or structurally owned, green
project rows, `2x` timing wins over `tsgo`, and measurable architecture debt
reduction.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh <AgentName>
scripts/agents/disk-preflight.sh <AgentName>
scripts/agents/list-owned-work.sh <AgentName>
```

## Current Assignment

- Primary gate:
- Bug or metric family:
- Architecture cleanup metric and command/counter:
- First live command to run:
- Next concrete step:

## Existing Work To Inspect First

- Live owned PRs from `scripts/agents/list-owned-work.sh <AgentName>`.
- Open issues with the lane's subsystem labels.
- Recent merged PRs touching the same invariant.
- Current dashboard/artifact data for the lane's release gate.

## Non-Overlap Rules

- Complete, close with evidence, or hand off live lane PRs before new issue
  work.
- Do not duplicate another active PR's invariant. Comment there instead.
- If you take over, leave a signed comment and update `agent:*` labels.
- State the structural rule; never patch one test name, source spelling,
  rendered type string, or fixture path.
- Architecture cleanup must ratchet down a named metric or unblock a release
  gate.

## Verification

- Prefer narrow unit, integration, artifact, or dashboard checks that answer the
  risk.
- Use `cargo nextest run` instead of `cargo test`.
- Do not run full conformance, full emit, full fourslash, or broad benchmarks
  locally.
- Wrap heavy commands with `scripts/safe-run.sh`.
