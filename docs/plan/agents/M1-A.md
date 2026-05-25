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
- 2026-05-25 18:55 UTC lane refresh:
  - Direct `agent:M1-A` PR queue is empty after `#9465` merged.
  - `#9465` landed on 2026-05-25 as
    `839abb594d test(checker): pin Record<TemplateLiteralPattern,V>
    excess-property check (#8725)`. Its synthetic queue branch
    `automation/merge-queue/pr-9465` was deleted after merge.
  - `#9559` is merged; the former M1-A JSX branch is no longer an active
    landing target.
  - `#10156` merged the queue-cleanup improvement. The cleanup tool may now
    delete superseded suffixed queue branches for open PRs when the suffix no
    longer matches current `main`; the latest dry run reports zero stale queue
    branches and preserves no active queue runs.
  - `#9230` merged on 2026-05-25 and is no longer an active queue unblocker.
  - The 2026-05-21 ready queue (`#9828`, `#9827`, `#9814`, `#9808`,
    `#9804`, `#9799`) is fully merged.
  - The old WIP-title queue (`#9822`, `#9803`, `#9639`) is resolved by merge
    or closure.
  - Agent label audit is clean: no missing, multiple, or noncanonical
    `agent:*` labels on open PRs.
  - WIP-state comment audit is clean.
- Current PR-garden surfaces to inspect before issue backlog:
  - `#10150` merged on 2026-05-25 and is no longer an active queue unblocker.
    Its stale synthetic branch was cleaned after merge.
  - Priority ready main-based PRs with `mergeStateStatus=BLOCKED` but
    `mergeable=MERGEABLE` include `#9632`, `#9912`, `#10078`, `#10081`,
    `#10084`, `#10085`, `#10087`, `#10126`, and `#10147`. These currently
    belong to other lanes; do not take them over unless the owner asks or a
    stale branch needs a signed handoff.
  - Queue branch cleanup currently skips open PR branches
    `automation/merge-queue/pr-10078`, `pr-10084`, `pr-10085`, `pr-10147`,
    `pr-9632`, `pr-9848`, and `pr-9912`.
  - Queue branch cleanup dry runs should use
    `--cleanup-superseded-open-queue-branches` so obsolete suffixed open-PR
    branches do not accumulate.
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
