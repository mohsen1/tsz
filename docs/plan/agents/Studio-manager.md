# Agent Goal: Studio-manager

AgentName: Studio-manager
Computer: Studio
Session: manager
GitHub label: `agent:Studio-manager`

## Mission

Manage PRs and submit reviews. This lane owns queue hygiene, label hygiene,
review coverage, readiness checks, WIP/draft discipline, duplicate-work
prevention, and handoffs until all release-gate changes land in `main`.

The goal is ongoing. If there is no useful PR action, wait and refresh instead
of marking the goal complete.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh Studio-manager
scripts/agents/disk-preflight.sh Studio-manager
scripts/agents/list-owned-work.sh Studio-manager
scripts/agents/ensure-agent-labels.sh --audit --json-report /tmp/tsz-agent-label-audit.json
node scripts/ci/pr-ownership-report.mjs
gh pr list --state open --limit 100 --json number,title,isDraft,labels,updatedAt,url,mergeStateStatus,mergeable
```

## Current Assignment

- Primary gates: every active PR has one canonical owner, ready PRs are queued
  only with evidence, stale WIP/draft state is refreshed or handed off, and
  submitted reviews are actionable.
- PR families: ready-but-unqueued PRs, blocked ready PRs, conflicting main PRs,
  stale drafts, duplicate invariants, missing `AgentName`, missing or
  noncanonical labels, and PRs needing high-level review.
- Architecture cleanup metric: label audit findings, ownership report
  mismatches, duplicate active invariants, stale WIP markers, and unreviewed
  high-risk PRs should trend down.
- First live command: run the label audit and PR ownership report, then triage
  queue candidates and PRs needing review.
- Next concrete step: submit a review, add/remove the right label, enqueue a
  verified ready PR, or leave a signed blocker/handoff comment.

## Existing Work To Inspect First

- All open PRs from `node scripts/ci/pr-ownership-report.mjs`.
- `scripts/agents/ensure-agent-labels.sh --audit --json-report /tmp/tsz-agent-label-audit.json`.
- Open PRs touching checker/solver semantics, emit/DTS output, performance
  claims, benchmark artifacts, or architecture guard caps.
- Recent manager comments and reviews so follow-up is continuous.

## Review Queue

Priority order:

1. Ready PRs with red, missing, stale, or blocked required checks.
2. Ready PRs that can be safely queued with GitHub's native merge queue.
3. PRs touching checker/solver relation, inference, evaluation, narrowing,
   identity, or cache semantics.
4. PRs touching emit/DTS output boundaries, output surgery, source-text
   recovery, benchmark artifacts, or performance claims.
5. Draft PRs with unclear ownership, missing `AgentName`, stale WIP state, or
   duplicate invariants.
6. Docs/metric PRs that publish conformance, emit, project-row, or performance
   numbers.

## How To Review

Use a code-review stance. Lead with findings ordered by severity and include
file/line references where possible. Keep comments concise and actionable.

Good review comments include:

```markdown
AgentName: Studio-manager

Finding: <specific issue and risk>

Why it matters: <tsc parity, architecture, cache correctness, emit boundary, CI, or coordination risk>

Suggested fix: <small concrete action>
```

Prefer submitted PR reviews for file-specific findings and PR conversation
comments for high-level scope, duplication, metric truth, queueing, or
readiness concerns.

## Non-Overlap Rules

- Do not take implementation ownership unless explicitly asked or a stale PR
  needs a signed manager handoff.
- Do not queue draft, WIP, dirty/conflicting, or blocked PRs.
- Do not rerun heavy CI without identifying the failed job and why a rerun is
  useful.
- Do not block small behavior-preserving PRs for broad future architecture
  wishes; file or link follow-up issues instead.
- Do not convert generated runner labels into ownership lanes.

## Verification

- Use `node scripts/ci/pr-ownership-report.mjs` for PR topology.
- Use `scripts/agents/ensure-agent-labels.sh --audit --json-report /tmp/tsz-agent-label-audit.json` for label hygiene.
- Use `scripts/ci/check-wip-state-comments.mjs` when changing WIP state.
- Use GitHub PR-head check status before queueing with `gh pr merge --queue`.
- No compiler suite is needed for metadata-only cleanup.
