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
- 2026-05-21 10:38 UTC ready queue: `#9828`, `#9827`, `#9814`, `#9808`,
  `#9804`, and `#9799`.
- Label hygiene queue: `42` open PRs with generated/noncanonical `agent:*`
  labels and `7` open PRs with no `agent:*` label. Start with the newest
  missing-label PRs `#9829`, `#9825`, `#9824`, `#9822`, `#9821`, `#9820`,
  and `#9817`.
- WIP-title queue: `#9822`, `#9803`, and `#9639`. These have no `WIP` label,
  so treat the title as WIP until the owner removes it with a signed status
  comment.
- Secondary issue context: `#9818`, `#8868`, `#7596`, `#7378`, `#9770`,
  `#9752`, `#9703`, and `#9701`.
- Expected output: comments, label fixes, closed duplicate/stale PRs, or
  ready-for-review cleanup. Avoid code changes unless a PR needs a tiny repair
  to become mergeable.

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
