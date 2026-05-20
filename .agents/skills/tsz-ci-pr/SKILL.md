---
name: tsz-ci-pr
description: Drive TSZ GitHub PRs through review, CI, and merge. Use when checking PR status, debugging failing GitHub Actions, deciding whether a PR is WIP, marking draft PRs ready, enabling or verifying auto-merge, landing a PR, or interpreting TSZ CI gates.
---

# TSZ CI And PR

Use this skill for TSZ PR operations. The repository uses draft PRs for
coordination and heavy CI only after ready-for-review. A queued merge is not a
landed merge; always verify the final PR state.

## Ground Rules

- Use `gh` for GitHub operations.
- Include the current `AgentName` in PR bodies and substantive comments.
- Never merge a PR that is draft, has a `WIP` label, starts with `[WIP]`, or
  says it is WIP in the title/body/branch description.
- Do not add `[codex]` to PR titles.
- Draft PRs run light CI. Ready PRs run conformance, emit, fourslash, WASM, and
  snapshot gates.
- If the user asks to land a PR, do not stop at enabled auto-merge. Verify
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

5. Use squash merge or squash auto-merge according to branch protection:

   ```bash
   gh pr merge <pr> --squash --auto
   ```

6. After checks pass, verify the merge actually happened:

   ```bash
   gh pr view <pr> --json state,mergedAt,mergedBy,url
   ```

If auto-merge is enabled but the PR remains open after green checks, inspect
branch protection and merge queue status, then run a direct squash merge when
allowed.

## CI Failure Triage

- `CI Summary` is the required umbrella status; inspect its prerequisites.
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
