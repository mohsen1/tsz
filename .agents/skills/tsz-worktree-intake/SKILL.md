---
name: tsz-worktree-intake
description: Start TSZ work safely in a crowded multi-agent checkout. Use when beginning a TSZ task, resuming a goal, switching or creating branches/worktrees, recovering from a merged or stale branch, checking disk/cache state, reading lane goals, or deciding whether local dirty files belong to the requested work.
---

# TSZ Worktree Intake

Use this skill before non-trivial TSZ work. The goal is to avoid wasted cycles
from stale branches, hidden dirty files, unowned PR overlap, missing TypeScript
corpus state, and cache-destroying cleanup.

## Fast Path

1. Pick or confirm a stable `AgentName`.
2. Refresh the remote truth:

   ```bash
   git fetch origin main
   ```

3. Read the durable direction and lane state:

   ```bash
   sed -n '1,260p' docs/plan/ROADMAP.md
   scripts/agents/show-goal.sh <AgentName>
   ```

4. Check local workspace health:

   ```bash
   git status --short --branch
   scripts/agents/disk-preflight.sh <AgentName>
   git worktree list
   ```

5. Check coordination overlap before editing:

   ```bash
   scripts/agents/list-owned-work.sh <AgentName>
   gh pr list --state open --limit 100 --json number,title,isDraft,headRefName,baseRefName,labels,updatedAt,url
   gh issue list --state open --limit 100 --json number,title,labels,updatedAt,url
   ```

Read `references/common-failure-modes.md` when any command reveals a stale
merged branch, dirty workspace, low disk, missing TypeScript corpus, or
overlapping active PR.

## Branch Choice

Use the current branch only when it is the active PR branch for the requested
work. If the current branch's PR is merged, closed, unrelated, or heavily ahead
of `origin/main`, create a fresh branch from `origin/main`.

Prefer:

```bash
git switch -c codex/<short-scope>-<yyyymmdd> origin/main
```

If dirty files exist, inspect them before switching. Carry them only when they
belong to the requested work; otherwise leave them unstaged and do not include
them in the PR.

## Worktree Choice

Reuse an existing sister worktree when disk or TypeScript corpus state makes it
cheaper. Create new worktrees beside the checkout, not inside it:

```bash
git worktree add ../tsz-<short-scope> -b codex/<short-scope>-<yyyymmdd> origin/main
cd ../tsz-<short-scope>
scripts/setup/link-ts-submodule.sh
```

Do not run broad `du` archaeology or `cargo clean` as a reflex. If disk is low,
follow the cleanup ladder printed by `scripts/agents/disk-preflight.sh`.

## Intake Decisions

Before coding, state the next action in one sentence:

- continue an existing PR,
- start a fresh non-overlapping branch,
- hand off or unblock an assigned draft,
- file an issue instead of editing because the improvement is out of scope.

For behavior work, also state the structural rule and owning layer before
editing. For process or docs work, state the iteration bottleneck and the
evidence that it is recurring.

## Stop Conditions

Pause and gather stronger evidence when:

- more than one open PR claims the same issue or title scope,
- the current branch is merged but still contains local-only changes,
- local dirty files affect the same paths as the planned edit but their owner is
  unclear,
- the TypeScript corpus is missing and the task needs conformance, emit, or
  fourslash source data,
- a command would destroy caches or worktree state to solve a temporary problem.
