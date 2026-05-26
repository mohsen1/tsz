---
name: tsz-pr-coordination
description: Create, refresh, and publish TSZ pull requests with correct coordination state. Use when opening a draft PR, updating a PR body, applying canonical agent labels, changing WIP/draft/ready state, acknowledging review, handing off readiness to a manager agent, or making sure a TSZ PR satisfies AgentName, Project Corpus Impact, verification, and overlap rules.
---

# TSZ PR Coordination

Use this skill for TSZ PR state, especially before pushing a branch or changing
draft/WIP/ready status. It complements `tsz-ci-pr`: use this skill for the
coordination contract and `tsz-ci-pr` for check triage. Merge and auto-merge
coordination belongs to an explicit manager or queue-owner agent, not every
implementation agent.

## Required Identity

Use a stable `AgentName`. If assigned to a canonical lane, use that lane name
in PR bodies and substantive comments. Do not create generated runner labels
such as `agent:claude-*` or typo labels such as `agnet:*`.

Canonical ownership labels are documented in
`docs/plan/agents/README.md`. Apply at most one `agent:*` label to a PR.

## Role Split

Individual implementation agents own only their own PRs. They should:

- keep their PR body, labels, WIP/draft state, verification, and review replies
  current,
- mark their own PR ready only when implementation and verification are done and
  the lane permits it,
- leave a signed readiness or blocker note for the manager agent,
- avoid queue sweeps, merge decisions, or auto-merge changes on unrelated PRs.

Manager or queue-owner agents handle cross-PR coordination. They may:

- inspect the full PR queue and ownership state,
- decide merge order and dependency handoffs,
- enable or disable auto-merge when policy and exact-head checks allow it,
- merge ready PRs or leave signed blocker comments.

If you are not acting as the manager/queue owner, do not merge PRs or use
auto-merge as part of routine implementation work.

## Before Publishing

1. Inspect scope:

   ```bash
   git status --short --branch
   git diff --stat
   git diff --check
   ```

2. Confirm the branch and overlap:

   ```bash
   gh pr list --state open --limit 100 --json number,title,isDraft,headRefName,baseRefName,labels,updatedAt,url
   scripts/agents/ensure-agent-labels.sh --audit
   ```

3. Stage only files that belong to the requested PR. If dirty files are mixed,
   leave unrelated files unstaged.

## PR Body

Use the repo template and fill every section. Never rely on `--fill` alone.
Read `references/pr-body-checklist.md` when creating or materially editing a
body.

Minimum sections:

```markdown
## Agent
AgentName: <AgentName>

## Track
<roadmap track and PR type>

## Invariant
When <condition>, <expected behavior>; this PR changes <owner/surface>.

## Scope
- <files or systems>

## Project Corpus Impact
- Row: n/a
- Bug family: n/a
- Evidence: <why n/a or affected row evidence>

## Verification
- <commands or CI gates>

## Coordination Notes
- <overlap, dependencies, WIP state, follow-ups>
```

After creating or editing the PR, verify the remote body:

```bash
gh pr view <number> --json body
```

## WIP And Draft State

Treat a PR as WIP if it is draft, has a `WIP` label, starts with `[WIP]`, or
declares a blocker in the body. Do not mark ready or enable auto-merge until the
current head is genuinely ready.

Whenever adding `WIP`, adding `[WIP]`, or converting a PR back to draft because
it is blocked, immediately leave a signed PR comment with:

- `AgentName`,
- why the PR is WIP,
- current blocker or active work,
- next owner/action,
- verification already run.

## Ready Handoff

Before marking your own PR ready or handing it to the manager:

1. Remove WIP markers only after implementation and verification are complete.
2. Confirm the latest pushed head SHA is the one you reviewed.
3. Confirm the PR body has current verification and Project Corpus Impact.
4. Comment with remaining risks, dependencies, or a clear "ready for manager
   review/queue" note.

Do not use auto-merge as a CI watcher. Unless you are the manager/queue owner,
leave merge and auto-merge decisions to the manager after exact-head checks are
complete.

## Review Responses

When a substantive review arrives:

1. Read the comment or thread.
2. React or reply so the reviewer knows it was seen.
3. Leave a concise signed PR comment saying whether you fixed it, will fix it,
   or disagree and why.
4. Update the PR body when the review changes scope, risk, or verification.
