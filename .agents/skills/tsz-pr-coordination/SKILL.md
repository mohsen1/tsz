---
name: tsz-pr-coordination
description: Create, refresh, and publish TSZ pull requests with correct coordination state. Use when opening a draft PR, updating a PR body, applying canonical agent labels, changing WIP/draft/ready state, acknowledging review, checking auto-merge readiness, or making sure a TSZ PR satisfies AgentName, Project Corpus Impact, verification, and overlap rules.
---

# TSZ PR Coordination

Use this skill for TSZ PR state, especially before pushing a branch or changing
draft/WIP/ready status. It complements `tsz-ci-pr`: use this skill for the
coordination contract and `tsz-ci-pr` for check triage and landing.

## Required Identity

Use a stable `AgentName`. If assigned to a canonical lane, use that lane name
in PR bodies and substantive comments. Do not create generated runner labels
such as `agent:claude-*` or typo labels such as `agnet:*`.

Canonical ownership labels are documented in
`docs/plan/agents/README.md`. Apply at most one `agent:*` label to a PR.

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

## Ready And Auto-Merge

Before marking ready:

1. Remove WIP markers only after implementation and verification are complete.
2. Confirm the latest pushed head SHA is the one you reviewed.
3. Confirm required checks for that head are green, not pending or missing.
4. Confirm the PR body has current verification and Project Corpus Impact.

Do not use auto-merge as a CI watcher. If checks are pending or failing, leave
auto-merge off and comment with the blocker.

## Review Responses

When a substantive review arrives:

1. Read the comment or thread.
2. React or reply so the reviewer knows it was seen.
3. Leave a concise signed PR comment saying whether you fixed it, will fix it,
   or disagree and why.
4. Update the PR body when the review changes scope, risk, or verification.
