---
name: tsz-disk-cache-hygiene
description: Prevent TSZ machines and worktrees from running out of disk while preserving useful build caches. Use before creating worktrees, running large builds/tests, diagnosing low disk, cleaning artifacts, deciding whether to delete caches, or recovering space without slowing future Rust/TypeScript builds.
---

# TSZ Disk Cache Hygiene

Use when disk, cleanup, or new worktrees are involved. Preserve hot caches unless
the machine is at risk.

## Compact Checks

```bash
df -h .
scripts/setup/disk-worktree-guard.sh
scripts/agents/disk-preflight.sh
git worktree list
```

Reuse inactive sister worktrees before creating new ones.

## Cleanup Ladder

If low disk:

```bash
scripts/setup/disk-worktree-guard.sh --auto-prune
scripts/setup/clean.sh --quiet
```

Rerun the guard. Read `references/cleanup-ladder.md` before deleting worktrees,
running `scripts/setup/clean.sh --full`, or manually removing build dirs.

## Preserve

Keep `.target/`, `.target-bench/`, `target/`, populated/symlinked
`TypeScript/`, and package-manager caches unless corruption or emergency space
pressure proves otherwise. Do not run `cargo clean`, `rm -rf target`, or full
clean as routine hygiene.

Report only before/after guard summary, caches preserved/destroyed, and why any
worktree/cache removal was safe. Avoid giant recursive size listings.
