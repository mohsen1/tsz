# TSZ Cleanup Ladder

Use this reference when compact cleanup did not recover enough space.

## Level 0: Do Nothing Destructive

Use when disk is not low:

```bash
scripts/setup/disk-worktree-guard.sh
```

Prefer reusing existing cached worktrees instead of cleaning.

## Level 1: Auto-Prune Stale Incremental Cache

Use when disk is low or before a large run on a tight machine:

```bash
scripts/setup/disk-worktree-guard.sh --auto-prune
```

This prunes old Cargo incremental directories while keeping built artifacts that
make future compiles faster.

## Level 2: Cache-Preserving Clean

Use when Level 1 is not enough:

```bash
scripts/setup/clean.sh --quiet
```

This removes routine debris while preserving `.target`, `.target-bench`, and
`target`.

Preview first when unsure:

```bash
scripts/setup/clean.sh --dry-run
```

## Level 3: Remove Abandoned Worktrees

Use only after confirming the branch/PR owner and preserving any useful
findings in a PR comment, issue, or handoff note.

Check:

```bash
git worktree list
gh pr list --state open --limit 100 --json number,title,headRefName,labels,url
```

Prefer deleting worktrees that are inactive, merged, closed, duplicated, or
explicitly handed off. Do not delete active sibling-agent work to free space.

## Level 4: Full Cache Deletion

Use only as a last resort when the machine cannot proceed and Levels 1-3 are
insufficient.

Examples of destructive commands requiring deliberate justification:

```bash
scripts/setup/clean.sh --full
cargo clean
rm -rf target .target .target-bench
```

Before doing this, state what will be lost, why cache-preserving cleanup was not
enough, and which future build will pay the rebuild cost.

## Never Start With

- broad `du -sh *` dumps,
- recursive sorted disk archaeology,
- deleting `TypeScript/` copies without checking whether they are shared
  symlinks,
- deleting build caches just because a task is finished.
