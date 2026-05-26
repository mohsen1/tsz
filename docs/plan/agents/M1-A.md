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
- 2026-05-26 02:30 UTC lane refresh:
  - Direct `agent:M1-A` PR queue is empty after `#10190` merged.
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
  - `#10181` merged on 2026-05-26 as
    `352ccabeef ci: distinguish claimed issue refs in ownership report
    (#10181)`. `scripts/ci/pr-ownership-report.mjs` now keeps raw
    `issueRefs` for audit context but groups duplicate issue work by
    `claimedIssueRefs` from PR titles plus `Addresses`/`Fixes`/`Closes`/
    `Resolves` body claims. The latest live report still shows 64 open PRs,
    20 drafts, 44 ready PRs, 4 stacked children, zero missing `AgentName`
    entries, and zero AgentName/label mismatches, and now reports no duplicate
    draft cleanup targets because the previous five were incidental
    coordination references rather than duplicate issue claims.
  - `#10183` merged on 2026-05-26 as
    `0495520fb1 ci: summarize merge queue skip reasons (#10183)`.
    Verbose `scripts/ci/poor-mans-merge-queue.mjs --dry-run` output now shows
    a full `Skip Reason Counts` table before the capped per-PR details. The
    latest live dry run reports 44 PRs skipped because auto-merge is not armed
    and 16 skipped as draft PRs, with no queue-ready auto-merge candidate.
  - `#10185` merged on 2026-05-26 as
    `41fdc94314 ci: show active queue runs in cleanup report (#10185)`.
    Verbose queue-branch cleanup dry runs now include an `Active Queue Runs`
    table with branch, PR, run id, and run URL for preserved active runs.
  - `#10187` merged on 2026-05-26 as
    `61f7b41458 ci: summarize cleanup queue branch skips (#10187)`.
    Verbose queue-branch cleanup dry runs now include cleanup-specific
    `Skip Reason Counts`, grouping detailed rows such as open PR branches and
    active queue runs without losing per-branch evidence.
  - `#10190` merged on 2026-05-26 as
    `a5aac8d71a ci: report blocked ready PR ownership (#10190)`.
    `scripts/ci/pr-ownership-report.mjs` now includes a `Blocked Ready Main
    PRs` section with owner counts and JSON fields for ready main-based PRs
    whose `mergeStateStatus` is `BLOCKED`.
  - `#10193` merged on 2026-05-26 as
    `e115436a07 ci: report conflicting main PR ownership (#10193)`.
    `scripts/ci/pr-ownership-report.mjs` now includes a `Conflicting Main PRs`
    section and JSON fields for main-based PRs whose current head is dirty or
    conflicting, grouped by owner.
  - `#10195` merged on 2026-05-26 as
    `a1057e55d0 ci: summarize PR ownership by owner (#10195)`.
    `scripts/ci/pr-ownership-report.mjs` now includes an `Owner Summary`
    section and `ownerSummaries` JSON with per-owner open, ready, draft, WIP,
    stacked-child, blocked-ready, conflicting-main, and auto-merge counts.
  - `#10197` merged on 2026-05-26 as
    `006e6e596f ci: list WIP PR ownership (#10197)`.
    `scripts/ci/pr-ownership-report.mjs` now includes a `WIP PRs` section and
    `wipPrs`/`wipOwnerCounts` JSON so cleanup agents can inspect the exact WIP
    rows behind owner-level WIP counts.
  - `#10199` merged on 2026-05-26 as
    `b82cd0be7b ci: report conflicting ready PR ownership (#10199)`.
    `scripts/ci/pr-ownership-report.mjs` now includes a `Conflicting Ready
    Main PRs` section and `conflictingReadyMainPrs`/`conflictingReadyMainOwnerCounts`
    JSON for the ready-only subset of dirty or conflicting main-based PRs.
  - `#10201` merged on 2026-05-26 as
    `0b7d82391b ci: summarize conflicting ready ownership (#10201)`.
    The ownership report's `Owner Summary` now includes a `Conflicting ready`
    column and `ownerSummaries[].conflictingReadyMain` JSON field.
  - `#10203` merged on 2026-05-26 as
    `f66f66cef0 ci: show conflicting ready update dates (#10203)`.
    `Conflicting Ready Main PRs` rows now show `updated YYYY-MM-DD`, and
    conflicting-main JSON rows include `updatedAt` for stale-handoff triage.
  - `#10205` merged on 2026-05-26 as
    `a7acc60e67 ci: show blocked ready update dates (#10205)`.
    `Blocked Ready Main PRs` rows now show `updated YYYY-MM-DD`, and
    blocked-ready JSON rows include `updatedAt` so stale blocked-ready
    handoffs have the same quick age signal as conflicting-ready handoffs.
  - `#10207` merged on 2026-05-26 as
    `c5108c4623 ci: show oldest updated owner counts (#10207)`.
    Blocked-ready, conflicting-ready, and conflicting-main owner-count rows now
    show `oldest updated YYYY-MM-DD`, and their JSON owner-count rows include
    `oldestUpdatedAt` for owner-level stale-handoff triage.
  - `#10209` merged on 2026-05-26 as
    `ecfe1e98c6 ci: show active queue run status (#10209)`.
    Verbose queue-branch cleanup dry runs now show active queue-run status and
    start time in the `Active Queue Runs` table, so preserved active queue
    branches can be age-triaged without opening each Actions run.
  - `#10211` merged on 2026-05-26 as
    `c2c4259aeb ci: show owners in queue skip report (#10211)`.
    Verbose queue dry runs now include the canonical `agent:*` owner label in
    skipped-PR rows, so `auto-merge off` and `draft PR` blocks can be handed
    off by lane without cross-referencing the ownership report.
  - `#10156` merged the queue-cleanup improvement. The cleanup tool may now
    delete superseded suffixed queue branches for open PRs when the suffix no
    longer matches current `main`.
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
  - Use the ownership report's `Owner Summary` section for the current
    owner-by-owner workload and handoff view. Counts are live GitHub state and
    should be re-run each cycle, not copied into this lane note. The
    `Conflicting ready` column is the quick owner-level count of non-draft PRs
    that still need conflict handoff before queueing; the blocked/conflicting
    owner-count sections now show oldest-update dates for age triage.
  - Use the ownership report's `WIP PRs` section for the current WIP marker
    rows and owner counts before adding, removing, or handing off WIP state.
  - No queue-ready auto-merge PR is currently selected by
    `scripts/ci/poor-mans-merge-queue.mjs --dry-run`; earlier ready PRs are
    either drafts, not auto-merge armed, or already handed off. Use
    `--verbose` when triaging this surface; skipped PR rows include owner
    labels for direct lane handoff.
  - Use the ownership report's `Blocked Ready Main PRs` section for the current
    ready main-based `mergeStateStatus=BLOCKED` surface. GitHub refreshes this
    state asynchronously, so do not freeze the count in the lane note; re-run
    the report for current owner counts and rows. Do not take those PRs over
    unless the owner asks or a stale branch needs a signed handoff. The
    `updated` date in each row is the quick staleness signal.
  - Use the ownership report's `Conflicting Main PRs` section for the current
    dirty/conflicting main-based branch surface. Treat those rows as handoff
    evidence for the owning lane, not permission to take over implementation
    branches without an explicit request or stale-branch handoff comment.
  - Use the ownership report's `Conflicting Ready Main PRs` section when
    deciding which non-draft branch blockers need owner handoff before queueing.
    The `updated` date in each row is the quick staleness signal.
  - Queue branch cleanup currently skips open PR branches
    `automation/merge-queue/pr-10078`, `pr-10084`, `pr-10147`, `pr-9515`,
    `pr-9632`, and `pr-9912`. Recent cleanup dry runs report zero stale
    branches and group the six preserved branches as open PR branch skips or
    active queue runs with status/start time; the exact active-run subset
    changes as synthetic runs complete, so re-run the cleanup dry-run for
    current run ids and ages. The stale merged-PR queue branches for `#9848`,
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
