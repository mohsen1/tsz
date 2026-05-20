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
- First pass: ready PRs `#9314`, `#9313`, `#9307`, `#9304`, `#9298`,
  `#9297`, `#9287`, and `#9103`.
- Secondary issue context: `#8223`, `#8203`, `#8432`, `#7596`, `#7626`.
- Expected output: comments, label fixes, closed duplicate/stale PRs, or
  ready-for-review cleanup. Avoid code changes unless a PR needs a tiny repair
  to become mergeable.

## Existing Work To Inspect First

- Open PR count is high; run `gh pr list --state open --limit 500`.
- Recent merges may make drafts obsolete; inspect recent merged PRs before
  reviving a branch.
- Stacked PRs exist. Do not close a child before identifying its base branch.

## Non-Overlap Rules

- Do not take over implementation lanes unless the current owner explicitly
  asks for handoff or the branch is stale and you leave a signed comment.
- When removing or adding `WIP`, leave a PR comment with `AgentName: M1-A`,
  the reason, current blocker, and next owner/action.
- Prefer closing duplicate coordination PRs over opening new coordination PRs.

## Verification

- Use `scripts/ci/pr-ownership-report.mjs` for PR topology.
- Use `scripts/ci/check-wip-state-comments.mjs` when changing WIP state.
- Do not run compiler suites for metadata-only cleanup.
