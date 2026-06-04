---
name: tsz-pr-coordination
description: Create, refresh, and publish TSZ pull requests with correct coordination state. Use when opening a draft PR, updating a PR body, applying canonical agent labels, changing WIP/draft/ready state, acknowledging review, handing off readiness to a manager agent, or making sure a TSZ PR satisfies AgentName, Project Corpus Impact, verification, and overlap rules.
---

# TSZ PR Coordination

Use for PR body/state/label/review coordination. Use `tsz-ci-pr` for failing
checks and queue status.

## Rules

- Stable `AgentName` in PR bodies and substantive comments.
- Use canonical `agent:*` labels only from `docs/plan/agents/README.md`; at
  most one per PR.
- Implementation agents manage their own PRs. Manager/queue-owner agents handle
  cross-PR queueing and merge order.
- If you are not the manager/queue owner, do not merge or queue PRs as routine
  implementation work.

## Before Publish

```bash
git status --short --branch
git diff --stat
git diff --check
gh pr list --state open --limit 100 --json number,title,isDraft,headRefName,baseRefName,labels,updatedAt,url
scripts/agents/ensure-agent-labels.sh --audit
```

Stage only files in scope.

## PR Body

Never rely on `--fill` alone. Read `references/pr-body-checklist.md` when
creating or materially editing a body. Required sections:

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

Verify remote body after create/edit:

```bash
gh pr view <number> --json body
```

## WIP, Ready, Reviews

- WIP means draft, `WIP` label, `[WIP]` title, or blocker in body. Do not mark
  ready or queue.
- Any WIP/draft/blocker transition needs an immediate signed comment with
  reason, blocker/current work, next owner/action, and verification run.
- Before ready handoff: remove WIP markers, review latest pushed head, update
  body verification/Project Corpus Impact, and leave a signed readiness/risk
  note for manager review.
- For substantive reviews: read, react/reply, leave a signed fix/will-fix/
  disagree note, and update body if scope/risk/verification changed.
