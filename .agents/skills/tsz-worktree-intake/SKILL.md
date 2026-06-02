---
name: tsz-worktree-intake
description: Start TSZ work safely in a crowded multi-agent checkout. Use when beginning a TSZ task, resuming a goal, switching or creating branches/worktrees, recovering from a merged or stale branch, checking disk/cache state, reading lane goals, or deciding whether local dirty files belong to the requested work.
---

# TSZ Worktree Intake

Use before non-trivial TSZ work.

## Fast Path

```bash
git fetch origin main
sed -n '1,260p' docs/plan/ROADMAP.md
scripts/agents/show-goal.sh <AgentName>
git status --short --branch
scripts/agents/disk-preflight.sh <AgentName>
git worktree list
scripts/agents/list-owned-work.sh <AgentName>
gh pr list --state open --limit 100 --json number,title,isDraft,headRefName,baseRefName,labels,updatedAt,url
gh issue list --state open --limit 100 --json number,title,labels,updatedAt,url
```

If disk is low or worktree reuse is unclear, use `tsz-disk-cache-hygiene`.
Read `references/common-failure-modes.md` only for stale merged branches, dirty
same-path files, low disk, missing TS corpus, or overlapping active PRs.

## Branch/Worktree Decision

- Continue current branch only if it is the active PR branch for this work.
- Otherwise create a fresh branch from `origin/main`.
- Reuse inactive sister worktrees when useful. New worktrees go beside this
  checkout:

```bash
git worktree add ../tsz-<scope> -b codex/<scope>-<yyyymmdd> origin/main
cd ../tsz-<scope>
scripts/setup/link-ts-submodule.sh
```

Do not run broad `du`, `cargo clean`, or destructive cache cleanup as intake.

## State The Decision

Before editing, say one sentence: continue existing PR, start fresh branch,
handoff/unblock draft, or file an issue. For behavior work also state the
structural rule and owner layer; for process/docs work state the recurring cost
and evidence.

## Stop

Pause for stronger evidence if overlap is ambiguous, branch is merged with
local-only changes, dirty files affect your paths, TS corpus is needed but
missing, or the next command would destroy caches/state.
