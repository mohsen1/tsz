# Agent Goal: M1-Opus

AgentName: M1-Opus
Computer: M1
Session: Opus
GitHub label: `agent:M1-Opus`

## Mission

Own deep checker-facing architecture debt that blocks all tests, project-row
parity, conformance strictness, bug closure, or tech-debt burn-down. This lane
handles cross-cutting M1 problems that are too broad for M1-A or M1-B, then
turns them into scoped landed PRs.

This goal is not complete when a branch exists. Keep going until the scoped
change lands in `main`, then pick the next M1-Opus release-gate item.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh M1-Opus
scripts/agents/disk-preflight.sh M1-Opus
scripts/agents/list-owned-work.sh M1-Opus
scripts/agents/ensure-agent-labels.sh --audit --json-report /tmp/tsz-agent-label-audit.json
python3 scripts/conformance/query-conformance.py --dashboard
scripts/arch/check-checker-boundaries.sh
python3 scripts/arch/arch_guard.py
```

## Current Assignment

- Primary gates: all tests pass, conformance strictness is hard, checker-facing
  bugs are fixed or structurally owned, and checker architecture debt decreases
  with evidence.
- Bug or metric families: diagnostic hardcoding, checker/query-boundary
  leakage, oversized checker orchestration, flow/narrowing ownership,
  relation diagnostic reason propagation, accepted-regression strictness, and
  source/rendered-text diagnostic shortcuts.
- Architecture cleanup metric: checker boundary guard findings, accepted
  regressions, oversized checker modules, and source/rendered-string diagnostic
  shortcuts should trend down.
- First live command: inspect owned PRs, then compare conformance dashboard,
  checker boundary guard, and architecture guard output for one debt item that
  blocks a release gate.
- Next concrete step: create or update a draft PR that burns down one measured
  checker debt counter while preserving or improving focused parity tests.

## Existing Work To Inspect First

- Live `agent:M1-Opus` PRs and stale M1-labelled PRs that need takeover.
- M1-A/M1-B PRs or blockers asking for deep checker help.
- `docs/architecture/BOUNDARIES.md`,
  `docs/architecture/QUERY_BOUNDARY_INVENTORY.md`, and
  `docs/architecture/RELATION_REQUEST.md`.
- Conformance accepted-regression entries and issues labelled `tech-debt`,
  `checker`, `accepted-regression`, `false-positive`, or `false-negative`.

## Non-Overlap Rules

- Do not take ordinary M1-A/M1-B work unless it needs cross-cutting checker
  architecture or the owner asks for takeover.
- Do not modify solver policy/cache behavior without a stack or explicit
  handoff to M4-B or M4-Opus.
- Do not accept broad refactors without a release-gate metric, guard count, or
  test failure they directly reduce.
- Leave signed handoff comments when splitting work back to ordinary lanes.

## Verification

- Use focused checker tests, checker boundary guards, architecture guards, and
  conformance dashboard queries.
- Use `cargo nextest run -p tsz_checker -- <test-filter>` for Rust tests.
- Run `scripts/arch/check-checker-boundaries.sh` after query-boundary changes.
- Do not run broad suites locally unless the PR specifically changes the suite
  harness.
