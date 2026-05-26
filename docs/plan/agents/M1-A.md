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
- 2026-05-26 00:43 UTC lane refresh:
  - Direct `agent:M1-A` PR queue is empty after `#10179` merged.
  - `#9465` landed on 2026-05-25 as
    `839abb594d test(checker): pin Record<TemplateLiteralPattern,V>
    excess-property check (#8725)`. Its synthetic queue branch
    `automation/merge-queue/pr-9465` was deleted after merge.
  - `#9559` is merged; the former M1-A JSX branch is no longer an active
    landing target.
  - `#10160` refreshed this lane state after `#9465` landed and merged on
    2026-05-25 as `4b484e5fd6 docs(agents): refresh M1-A post-9465
    state (#10160)`. Its stale synthetic queue branch was deleted after merge.
  - `#10163` merged on 2026-05-25 as
    `25656f49fa ci: report stale run cancellation attempts (#10163)`. Its
    synthetic queue branch `automation/merge-queue/pr-10163` was deleted after
    the synthetic queue run `26422310092` completed successfully.
  - `#10166` merged on 2026-05-25 as
    `ceadbd07a3 ci: report active open queue branch runs (#10166)`. Queue
    cleanup dry runs now report active workflow runs on open queue branches as
    preserved instead of hiding them under ordinary open-PR skips.
  - `#10168` merged on 2026-05-25 as
    `6caa98667f ci: annotate ownership duplicate clusters (#10168)`.
    `scripts/ci/pr-ownership-report.mjs` now annotates duplicate title and
    issue clusters with draft/ready state, WIP marker, and AgentName so cleanup
    agents can triage ownership without opening every PR.
  - `#10170` merged on 2026-05-25 as
    `24936e167a ci: report ownership AgentName mismatches (#10170)`.
    `scripts/ci/pr-ownership-report.mjs` now reports open PRs where the body
    `AgentName` disagrees with the single canonical `agent:*` label; the latest
    live report now shows zero such mismatches after M1-A normalized the 15
    mismatched PR body `AgentName` lines to their existing canonical labels.
  - `#10173` merged on 2026-05-26 as
    `de76a344d0 ci: annotate ownership report stack roles (#10173)`.
    Duplicate issue clusters in `scripts/ci/pr-ownership-report.mjs` now mark
    stacked PRs as `stack root`, `stack middle`, or `stack child`; the latest
    live report shows 64 open PRs, 20 drafts, 44 ready PRs, 4 stacked children,
    zero missing `AgentName` entries, and zero AgentName/label mismatches.
  - `#10175` merged on 2026-05-26 as
    `98a8bc656c ci: classify duplicate issue draft stacks (#10175)`.
    The duplicate-issue section now classifies draft clusters as
    `stacked-only drafts`, `mixed stacked/unstacked drafts`, or
    `unstacked drafts`; the latest live report shows unstacked duplicate-draft
    cleanup targets on `#9694`, `#9809`, and `#9886`, plus mixed clusters on
    `#9634` and `#9904`.
  - `#10177` merged on 2026-05-26 as
    `3d049d81c5 ci: surface duplicate draft cleanup targets (#10177)`.
    `scripts/ci/pr-ownership-report.mjs` now has a dedicated
    `Duplicate Draft Cleanup Targets` section that filters out stacked-only
    draft chains and surfaces the five current unstacked/mixed cleanup targets
    directly.
  - `#10179` merged on 2026-05-26 as
    `965c5dd4d1 ci: export duplicate draft cleanup targets (#10179)`.
    The JSON report now exposes the same cleanup list as
    `duplicateDraftCleanupTargets`, so follow-up automation can consume the
    unstacked/mixed duplicate-draft target list without recomputing the
    markdown filter.
  - `#10156` merged the queue-cleanup improvement. The cleanup tool may now
    delete superseded suffixed queue branches for open PRs when the suffix no
    longer matches current `main`; the latest dry run reports zero stale queue
    branches and preserves no active queue runs.
  - `#9889` landed through the poor-man queue during M1-A queue drain. Its
    stale synthetic queue branch was deleted after merge.
  - `#9875` was selected by the queue but conflicted with current `main`.
    M1-A disabled auto-merge and left a signed M4-C handoff comment; leave it
    off auto-merge until the owner refreshes and re-verifies the branch.
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
  - No queue-ready auto-merge PR is currently selected by
    `scripts/ci/poor-mans-merge-queue.mjs --dry-run`; earlier ready PRs are
    either drafts, not auto-merge armed, or already handed off.
  - Priority ready main-based PRs with `mergeStateStatus=BLOCKED` but
    `mergeable=MERGEABLE` include `#9632`, `#9912`, `#10078`, `#10081`,
    `#10084`, `#10087`, `#10126`, and `#10147`. These currently
    belong to other lanes; do not take them over unless the owner asks or a
    stale branch needs a signed handoff.
  - Queue branch cleanup currently skips open PR branches
    `automation/merge-queue/pr-10078`, `pr-10084`, `pr-10147`, `pr-9515`,
    `pr-9632`, and `pr-9912`. The stale merged-PR queue branches for `#9848`,
    `#9889`, `#10160`, and `#10163` were deleted.
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
