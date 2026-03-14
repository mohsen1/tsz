#!/usr/bin/env bash
# =============================================================================
# cleanup.sh — Free disk space by removing stale artifacts and worktrees
# =============================================================================
#
# Usage:
#   scripts/session/cleanup.sh             # Interactive mode
#   scripts/session/cleanup.sh --auto      # Auto-clean without prompts
#   scripts/session/cleanup.sh --dry-run   # Show what would be cleaned
#
# Cleans:
#   1. target/ dirs in worktrees with no recent commits (>24h)
#   2. Worktrees for merged campaign branches
#   3. Stale remote-tracking branches
#   4. Cargo caches (if severely low on disk)
#
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"

AUTO=false
DRY_RUN=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --auto) AUTO=true; shift ;;
        --dry-run) DRY_RUN=true; shift ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

action() {
    if $DRY_RUN; then
        echo "  [DRY RUN] Would: $*"
    else
        "$@"
    fi
}

echo "============================================="
echo "TSZ Disk Cleanup — $(date '+%Y-%m-%d %H:%M')"
echo "============================================="

# --- Disk usage before ---
echo ""
echo "BEFORE:"
disk_free=$(df -h "$REPO_ROOT" 2>/dev/null | tail -1 | awk '{print $4}')
echo "  Disk free: $disk_free"

total_cleaned=0

# --- 1. Clean target/ in stale worktrees ---
echo ""
echo "1. STALE WORKTREE TARGETS:"
echo "   (worktrees with no commits in >24 hours)"
echo ""

for wt in "$REPO_ROOT/.worktrees"/*/; do
    [[ -d "$wt" ]] || continue
    wt_name=$(basename "$wt")

    if [[ -d "${wt}target" ]]; then
        # Check last commit age
        last_commit=$(git -C "$wt" log -1 --format="%ct" 2>/dev/null || echo "0")
        now=$(date +%s)
        age_hours=$(( (now - last_commit) / 3600 ))

        wt_size=$(du -sm "${wt}target" 2>/dev/null | cut -f1 || echo "0")

        if [[ "$age_hours" -gt 24 ]]; then
            echo "  $wt_name: ${wt_size}MB (${age_hours}h since last commit) — CLEANING"
            action rm -rf "${wt}target"
            total_cleaned=$((total_cleaned + wt_size))
        else
            echo "  $wt_name: ${wt_size}MB (${age_hours}h since last commit) — keeping"
        fi
    fi
done

# --- 2. Remove worktrees for merged branches ---
echo ""
echo "2. MERGED CAMPAIGN WORKTREES:"
echo ""

git -C "$REPO_ROOT" fetch origin --quiet 2>/dev/null || true

for wt_line in $(git -C "$REPO_ROOT" worktree list --porcelain 2>/dev/null | grep "^worktree " | sed 's/^worktree //'); do
    [[ "$wt_line" == "$REPO_ROOT" ]] && continue

    # Get the branch for this worktree
    wt_branch=$(git -C "$wt_line" branch --show-current 2>/dev/null || echo "")

    if [[ "$wt_branch" == campaign/* ]]; then
        # Check if merged into main
        if git -C "$REPO_ROOT" merge-base --is-ancestor "$wt_branch" origin/main 2>/dev/null; then
            wt_size=$(du -sm "$wt_line" 2>/dev/null | cut -f1 || echo "0")
            echo "  $wt_branch: merged into main — REMOVING (${wt_size}MB)"
            action git -C "$REPO_ROOT" worktree remove "$wt_line" --force
            action git -C "$REPO_ROOT" branch -d "$wt_branch"
            total_cleaned=$((total_cleaned + wt_size))
        else
            echo "  $wt_branch: not merged — keeping"
        fi
    fi
done

# --- 3. Prune stale remote-tracking branches ---
echo ""
echo "3. STALE REMOTE BRANCHES:"
echo ""
stale_count=$(git -C "$REPO_ROOT" remote prune origin --dry-run 2>/dev/null | grep -c "\[would prune\]" || echo "0")
if [[ "$stale_count" -gt 0 ]]; then
    echo "  $stale_count stale remote-tracking branches"
    action git -C "$REPO_ROOT" remote prune origin
else
    echo "  None found"
fi

# --- 4. Shared CARGO_TARGET_DIR cleanup ---
if [[ -n "${CARGO_TARGET_DIR:-}" ]] && [[ -d "$CARGO_TARGET_DIR" ]]; then
    echo ""
    echo "4. SHARED CARGO_TARGET_DIR:"
    shared_size=$(du -sm "$CARGO_TARGET_DIR" 2>/dev/null | cut -f1 || echo "0")
    echo "  $CARGO_TARGET_DIR: ${shared_size}MB"

    if [[ "$shared_size" -gt 20000 ]]; then  # >20GB
        echo "  WARNING: Very large. Consider running: cargo clean --target-dir $CARGO_TARGET_DIR"
    fi
fi

# --- Summary ---
echo ""
echo "============================================="
echo "Cleaned: ~${total_cleaned}MB"
disk_free_after=$(df -h "$REPO_ROOT" 2>/dev/null | tail -1 | awk '{print $4}')
echo "Disk free: $disk_free → $disk_free_after"
echo "============================================="
