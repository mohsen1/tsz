# Agent Goal: M4-B

AgentName: M4-B
Computer: M4
Session: B
GitHub label: `agent:M4-B`

## Mission

Consolidate relation policy and cache-key protocols so relation answers are
stable, explainable, and shared by checker diagnostics.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh M4-B
scripts/agents/disk-preflight.sh M4-B
scripts/agents/list-owned-work.sh M4-B
```

## Current Assignment

- Initial priority: land, close, or clearly hand off existing PRs in this lane
  before claiming issue backlog.
- Current open PRs owned by `agent:M4-B`:
  - `#10078` ready/off-auto; this lane-doc PR is docs-only. Direct squash
    merge is blocked by the protected-branch policy; queue runs invalidate
    `Queue Tested` after each synchronize and report no queue-ready auto-merge
    PR because auto-merge remains off by lane rule. Do not churn this PR just
    to update its own head SHA.
  - `#10058` ready/off-auto on rebased head
    `171fc3620611a4ba128b1d156f1ee8d739372bf1`; fresh CI is running to
    determine whether the earlier `conformance-aggregate` coverage gap
    (`11844 < 11865` tolerance 5) was stale-base/cache-related.
  - `#9945` ready/off-auto; exact-head ready-review CI is still running.
  - `#9807` draft/WIP/off-auto; follow-up head
    `023ac1dde31e330514196d178b11d3515f832814` splits visitor predicates below
    2000 LOC, but exact-head draft-light jobs are still running.
  - `#9230` ready/off-auto; exact-head draft-light CI passed and M4-B promoted
    the PR to ready review on
    `dd48ce95538d367106e470ac025fa0bb8bd6f141`.
- Completed relation-policy stack state: `#9265`, `#9268`, and `#9650` are
  merged; `#9289` is closed. Do not reopen or duplicate these without a fresh
  reason.
- Older draft/new-issue cluster references to inspect only after the open PRs
  above are landed, closed, or explicitly handed off: `#9798`, `#8207`, and
  `#8203`. `#9803` and `#9800` are closed.
- Track: roadmap Tracks 3, 4, and 10.
- Next concrete step: inspect queued exact-head CI for the open PR set above.
  If a PR is green and not draft/WIP/blocked, mark or keep it ready and land it
  according to the TSZ CI rules. If a PR is draft but light CI is clean and the
  body/comment handoff says its blocker is fixed, mark it ready for heavy CI
  instead of adding more scope.

## Existing Work To Inspect First

- `#9281` is no longer owned by `agent:M4-B`; inspect only for stack context,
  not as an M4-B lane PR.
- `#9807` is the remaining draft/WIP PR that should be advanced or handed off
  before taking issue backlog.
- `#9230`, `#9945`, `#10058`, and `#10078` are ready/off-auto and should be
  landed only after exact-head required checks are complete and green. For
  `#10078`, required `Queue Tested` is still pending because auto-merge is not
  armed; do not arm it under the lane rules while a required status is pending.
  `#10058` is waiting on fresh CI after its rebase.
- M1-B depends on this lane for checker relation gateway cleanup.
- `#9798` is owned by `agent:M4-C`; inspect only for overlap and do not take
  ownership unless explicitly handed off.

## Non-Overlap Rules

- Cache keys must include every semantic mode that can change relation answers.
- Do not combine broad performance pre-sizing with semantic policy changes.
- If a checker call site needs only routing, hand off to M1-B.

## Verification

- Prefer targeted solver tests that compare cache-enabled and cache-disabled
  behavior where available.
- Record behavior unchanged for pure refactors.
- Use `cargo nextest run`, not `cargo test`.
