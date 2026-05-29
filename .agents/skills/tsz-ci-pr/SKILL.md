---
name: tsz-ci-pr
description: Drive TSZ GitHub PRs through review, CI, and the merge queue. Use when checking PR status, debugging failing GitHub Actions, deciding whether a PR is WIP, marking draft PRs ready, enqueueing or verifying the queue, landing a PR, or interpreting TSZ CI gates.
---

# TSZ CI And PR

Use this skill for TSZ PR operations. The repository uses draft PRs for
coordination and heavy CI only after ready-for-review. TSZ's required
`Queue Tested` status is produced by the repo-local merge queue after a ready
PR is labeled `merge-queue`. A queued merge is not a landed merge; always
verify the final PR state.

## Ground Rules

- Use `gh` for GitHub operations.
- Include the current `AgentName` in PR bodies and substantive comments.
- Never merge a PR that is draft, has a `WIP` label, starts with `[WIP]`, or
  says it is WIP in the title/body/branch description.
- Do not add `[codex]` to PR titles.
- Draft PRs run light CI. Ready PRs run conformance, emit, fourslash, WASM, and
  snapshot gates.
- `Queue Tested` is queue-owned. It may be missing or pending before enqueue;
  do not wait for it before labeling an otherwise ready PR `merge-queue`.
- If the user asks to land a PR, do not stop at an armed queue. Verify
  `state: MERGED` or keep working.

## Status Commands

Inspect merge readiness:

```bash
gh pr view <pr> --json state,isDraft,mergeStateStatus,mergeable,autoMergeRequest,mergedAt,labels,title,url,headRefName,headRefOid
gh pr checks <pr> --json name,state,bucket,link,completedAt
```

Watch checks only when the result is immediately needed:

```bash
gh pr checks <pr> --watch --interval 20
```

Inspect failed logs:

```bash
gh run view <run-id> --log-failed
gh run view <run-id> --job <job-id> --log
```

## Landing Workflow

1. Confirm the PR is not WIP by state, labels, title, body, and branch context.
2. Confirm the head SHA matches the checks being inspected.
3. If checks fail, fix the root cause, push, and comment with what changed.
4. When implementation and verification are complete, mark ready if still draft:

   ```bash
   gh pr ready <pr>
   ```

5. Once PR-head checks such as `CI Summary` and `GitGuardian Security Checks`
   pass for the exact head, enqueue the PR with the durable queue label:

   ```bash
   gh pr edit <pr> --add-label merge-queue
   ```

6. Watch `Queue Tested` only as queue evidence, not as a PR-head prerequisite.
   The `Poor Man's Merge Queue` workflow synthetic-tests one latest-`main`
   merge branch and posts `Queue Tested` to the PR head before merging.
7. After the queue runs, verify the merge actually happened:

   ```bash
   gh pr view <pr> --json state,mergedAt,mergedBy,url
   ```

If a `merge-queue` labeled PR remains open after green PR-head checks, inspect
`Queue Tested`, the `Poor Man's Merge Queue` workflow, and any synthetic
`automation/merge-queue/pr-<n>` CI run. Do not direct-merge around the queue
unless the user explicitly asks for an emergency admin bypass.

## CI Failure Triage

- `CI Summary` is the required umbrella PR-head status; inspect its
  prerequisites.
- `Queue Tested` is the required queue status; inspect the queue workflow and
  synthetic merge branch when it fails or stays pending.
- `conformance-aggregate` can fail even when shards pass if accepted-regression
  drift exists. Compare unlisted vs resolved accepted paths.
- `unit-cloudbuild` often exposes memory, linking, or architecture-contract
  failures that local fast tests may miss.
- Docs-only and bench-shell-only paths intentionally short-circuit most jobs.
- Ready-review CI failures should be fixed in the PR unless the PR is explicitly
  abandoned or converted back to draft.

## PR Comment Shape

Use concise comments:

- `AgentName: <name>`
- root cause,
- files changed,
- verification or CI run URL,
- remaining risk or follow-up issue.
