# Agent Goal: M1-A

AgentName: M1-A
Computer: M1
Session: A
GitHub label: `agent:M1-A`

## Mission

Drain the PR garden and keep active work legible. This lane moves the roadmap's
Phase 0 runway forward by closing stale branches, removing incorrect `WIP`
state, and making sure every active PR has one next owner.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh M1-A
scripts/agents/disk-preflight.sh M1-A
scripts/agents/list-owned-work.sh M1-A
node scripts/ci/pr-ownership-report.mjs
```

## Current Assignment

- Primary lane: PR readiness, stale-WIP cleanup, and ownership label hygiene.
- 2026-05-25 14:05 UTC lane refresh:
  - Direct `agent:M1-A` PR queue is empty after `#10154` merged.
  - The 2026-05-21 ready queue (`#9828`, `#9827`, `#9814`, `#9808`,
    `#9804`, `#9799`) is fully merged.
  - The old WIP-title queue (`#9822`, `#9803`, `#9639`) is resolved by merge
    or closure.
  - Agent label audit is clean: no missing, multiple, or noncanonical
    `agent:*` labels on open PRs.
  - WIP-state comment audit is clean.
- Current PR-garden surfaces to inspect before issue backlog:
  - Ready main-based PRs with `mergeStateStatus=BLOCKED` but
    `mergeable=MERGEABLE`: `#9230`, `#9281`, `#9634`, `#9807`, `#9811`,
    `#9912`, `#10078`, `#10081`, `#10086`, `#10126`, and `#10150`.
  - Ready main-based PRs with `mergeable=CONFLICTING`: `#9632` and `#10084`.
  - Open queue branches should be cleaned only when their PR is merged or
    closed; the latest cleanup dry run preserved only open PR branches.
- Secondary issue context remains `#9818`, `#8868`, `#7596`, `#7378`, `#9770`,
  `#9752`, `#9703`, and `#9701`.
- Expected output: comments, label fixes, closed duplicate/stale PRs, or
  ready-for-review cleanup. Avoid code changes unless a PR needs a tiny repair
  to become mergeable. Do not take over another lane's implementation PR unless
  it is stale and you leave a signed handoff/status comment first.

## Existing Work To Inspect First

- Open PR count is high; run `gh pr list --state open --limit 500`.
- Recent merges may make drafts obsolete; inspect recent merged PRs before
  reviving a branch.
- Stacked PRs exist. Do not close a child before identifying its base branch.
- Run `scripts/agents/ensure-agent-labels.sh --audit` before every broad
  label pass; generated Claude Code labels are runner metadata, not lanes.
- The issue backlog expanded by 136 new issues on 2026-05-21. Do not assign
  `agent:*` labels to issues yet; keep issue work clustered through PRs.

## Non-Overlap Rules

- Do not take over implementation lanes unless the current owner explicitly
  asks for handoff or the branch is stale and you leave a signed comment.
- When removing or adding `WIP`, leave a PR comment with `AgentName: M1-A`,
  the reason, current blocker, and next owner/action.
- Prefer closing duplicate coordination PRs over opening new coordination PRs.

## Verification

- Use `scripts/ci/pr-ownership-report.mjs` for PR topology.
- Use `scripts/ci/check-wip-state-comments.mjs` when changing WIP state.
- Use `scripts/agents/ensure-agent-labels.sh --audit` for generated-label
  drift before and after cleanup.
- Do not run compiler suites for metadata-only cleanup.
