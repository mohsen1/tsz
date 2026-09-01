---
name: tsz-pr-coordination
description: Create, refresh, and publish TSZ pull requests with correct body and state. Use when opening a PR, updating a PR body, changing WIP/draft/ready state, acknowledging review, or making sure a TSZ PR satisfies the Goal, Verification, Provenance, and overlap rules.
---

# TSZ PR Coordination

Use for PR body/state/review coordination. Use `tsz-ci-pr` for failing
checks and queue status.

## Rules

- Every PR serves one roadmap goal: `green`, `fast`, `grow`, or `hold`.
- The PR author manages and lands their own PR; there is no manager or
  reviewer lane.
- Land-and-continue: push, start the next task, and return to queue the
  merge when exact-head checks resolve. Never idle-wait on CI.

## Before Publish

```bash
git status --short --branch
git diff --stat
git diff --check
gh pr list --state open --limit 100 --json number,title,isDraft,headRefName,baseRefName,labels,updatedAt,url
```

Stage only files in scope.

## PR Body

Never rely on `--fill` alone. Read `references/pr-body-checklist.md` when
creating or materially editing a body. This is a reviewer/future-session
contract; there is no `pr-body-gate` CI job:

```markdown
Goal: <green|fast|grow|hold>
## Verification
- <commands or CI gates>
## Provenance
Machine: <m1|m4|studio|cloud|hostname>
Assistant: <claude-code|codex|bot>
Model: <model id, e.g. claude-opus-4-8>
Effort: <low|medium|high|max>
```

Report your actual runtime values (machine, assistant, model, effort); do not
invent nicknames.

Verify remote body after create/edit:

```bash
gh pr view <number> --json body
```

## WIP, Ready, Reviews

- WIP means draft, `WIP` label, `[WIP]` title, or blocker in body. Do not mark
  ready or queue.
- Any WIP/draft/blocker transition needs an immediate comment with reason,
  blocker/current work, next action, and verification run, signed with a
  provenance line (e.g. `Provenance: studio / claude-code / claude-opus-4-8 /
  high`).
- Before marking ready: remove WIP markers, review the latest pushed head, and
  update the body's `## Verification` section to match it.
- For substantive reviews: read, react/reply, leave a fix/will-fix/disagree
  note signed with a provenance line, and update body if scope/risk/
  verification changed.
