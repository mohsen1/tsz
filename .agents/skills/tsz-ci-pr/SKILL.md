---
name: tsz-ci-pr
description: Drive TSZ GitHub PRs through review, CI, and the merge queue. Use when checking PR status, debugging failing GitHub Actions, deciding whether a PR is WIP, marking draft PRs ready, enqueueing or verifying the queue, landing a PR, or interpreting TSZ CI gates.
---

# TSZ CI And PR

Use for PR checks, CI triage, ready state, queue evidence, and landing. Use
`tsz-pr-coordination` for PR body/label contract.

## Rules

- Use `gh`; sign substantive comments with `AgentName`.
- Never merge draft/WIP PRs (`draft`, `WIP`, `[WIP]`, or body/branch says WIP).
- Ready PRs run heavy CI. Draft PRs intentionally run light CI.
- `Queue Tested` is queue-owned and appears after `merge-queue`.
- If asked to land, verify `state: MERGED`; queue label alone is not enough.

## Inspect

```bash
gh pr view <pr> --json state,isDraft,mergeStateStatus,mergeable,autoMergeRequest,mergedAt,labels,title,url,headRefName,headRefOid
gh pr checks <pr> --json name,state,bucket,link,completedAt
gh run view <run-id> --log-failed
gh run view <run-id> --job <job-id> --log
```

Use `gh pr checks <pr> --watch --interval 20` only when the result is needed
now.

## Landing

1. Confirm not WIP and head SHA matches inspected checks.
2. Fix failures in-PR; comment with root cause and verification.
3. Mark ready only after implementation and verification:
   `gh pr ready <pr>`.
4. Add `merge-queue` only when exact-head PR-head checks such as `CI Summary`
   and `GitGuardian Security Checks` pass and policy permits:
   `gh pr edit <pr> --add-label merge-queue`.
5. Inspect `Poor Man's Merge Queue` and synthetic
   `automation/merge-queue/pr-<n>` runs if queue stalls.
6. Verify final merge:
   `gh pr view <pr> --json state,mergedAt,mergedBy,url`.

## Failure Hints

- `CI Summary`: umbrella PR-head status.
- `Queue Tested`: queue synthetic-merge status.
- `conformance-aggregate`: accepted-regression drift can fail aggregate even
  when shards pass.
- `unit-cloudbuild`: often exposes memory, linking, or architecture guard
  failures.
- Docs-only and bench-shell-only paths short-circuit most jobs.

Comment shape: `AgentName`, root cause, files changed, verification/CI URL,
remaining risk.
