---
name: tsz-disk-cache-hygiene
description: Prevent TSZ machines and worktrees from running out of disk while preserving useful build caches. Use before creating worktrees, running large builds/tests, diagnosing low disk, cleaning artifacts, deciding whether to delete caches, or recovering space without slowing future Rust/TypeScript builds.
---

# TSZ Disk Cache Hygiene

Use this skill whenever disk space or cleanup is part of the task. The priority
order is: keep the machine usable, preserve hot build caches when possible, and
avoid broad disk archaeology that burns time and terminal output.

## Default Workflow

1. Start with compact signals:

   ```bash
   df -h .
   scripts/setup/disk-worktree-guard.sh
   scripts/agents/disk-preflight.sh <AgentName>
   git worktree list
   ```

2. Reuse an inactive worktree before creating a new one, especially when it
   already has `.target`, `target`, `.target-bench`, or `TypeScript` state.
3. If disk is low, run the cache-preserving ladder in order:

   ```bash
   scripts/setup/disk-worktree-guard.sh --auto-prune
   scripts/setup/clean.sh --quiet
   ```

4. Rerun the compact guard to see whether enough space was recovered.
5. Escalate only when the guard still reports low space.

Read `references/cleanup-ladder.md` before deleting worktrees, running
`scripts/setup/clean.sh --full`, or removing build directories manually.

## Cache Policy

Preserve by default:

- `.target/`
- `.target-bench/`
- `target/`
- populated or symlinked `TypeScript/`
- package-manager caches unless the task is specifically about dependency
  corruption

These are expensive to rebuild and directly affect iteration speed. Do not run
`cargo clean`, `rm -rf target`, or `scripts/setup/clean.sh --full` as routine
hygiene.

Safe routine cleanup:

- stale Cargo `incremental` subdirectories older than seven days,
- gitignored debris outside protected build caches,
- abandoned worktrees after confirming their branch/PR owner and preserving
  useful findings.

## New Worktree Decision

Before `git worktree add`, answer:

- Is there an inactive sister worktree with the right cache shape?
- Does the disk guard show enough free space?
- Does the task need a real TypeScript corpus, or can it run without it?
- Can a sibling worktree link to an existing TypeScript checkout instead of
  creating another copy?

Preferred new-worktree setup:

```bash
git worktree add ../tsz-<short-scope> -b codex/<short-scope>-<yyyymmdd> origin/main
cd ../tsz-<short-scope>
scripts/setup/link-ts-submodule.sh
```

## What To Report

When cleanup or reuse affects the work, include:

- the guard output summary before/after,
- whether caches were preserved,
- any abandoned worktree removed and why it was safe,
- any deliberate cache destruction and why no safer option was enough.

Do not include giant recursive size listings unless a targeted cleanup needs
exact ownership.
