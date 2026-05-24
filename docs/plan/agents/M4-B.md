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
  - `#10058` ready/off-auto; exact-head CI still had pending jobs in the
    2026-05-24 M4-B audit.
  - `#9945` ready/off-auto; split-head draft-light CI passed and ready-review
    CI was queued in the 2026-05-24 M4-B audit.
  - `#9807` ready/off-auto; ready-review CI was green except pending
    `Queue Tested` in the 2026-05-24 M4-B audit.
  - `#9230` draft/off-auto; follow-up head
    `dd48ce95538d367106e470ac025fa0bb8bd6f141` fixed the focused
    `coAndContraVariantInferences` and `intraExpressionInferences` blockers
    locally, then left exact-head draft-light CI queued.
- Completed relation-policy stack state: `#9265`, `#9268`, and `#9650` are
  merged; `#9289` is closed. Do not reopen or duplicate these without a fresh
  reason.
- Older draft/new-issue cluster references to inspect only after the open PRs
  above are landed, closed, or explicitly handed off: `#9803`, `#9800`,
  `#9798`, `#8207`, and `#8203`.
- Track: roadmap Tracks 3, 4, and 10.
- Next concrete step: inspect queued exact-head CI for the open PR set above.
  If a PR is green and not draft/WIP/blocked, mark or keep it ready and land it
  according to the TSZ CI rules. If a PR is draft but light CI is clean and the
  body/comment handoff says its blocker is fixed, mark it ready for heavy CI
  instead of adding more scope.

## Existing Work To Inspect First

- `#9281` is no longer owned by `agent:M4-B`; inspect only for stack context,
  not as an M4-B lane PR.
- `#9230` is the remaining draft PR that should be advanced or handed off before
  taking issue backlog.
- `#9945`, `#9807`, `#10058`, and `#10078` are ready/off-auto and should be
  landed only after exact-head required checks are complete and green.
- M1-B depends on this lane for checker relation gateway cleanup.
- `#9803` is titled `[WIP]`; keep it WIP until the owner leaves a signed
  status comment and removes the title prefix.

## Non-Overlap Rules

- Cache keys must include every semantic mode that can change relation answers.
- Do not combine broad performance pre-sizing with semantic policy changes.
- If a checker call site needs only routing, hand off to M1-B.

## Verification

- Prefer targeted solver tests that compare cache-enabled and cache-disabled
  behavior where available.
- Record behavior unchanged for pure refactors.
- Use `cargo nextest run`, not `cargo test`.
