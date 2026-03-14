#!/usr/bin/env bash
# =============================================================================
# integrate.sh — Validate and merge campaign branches to main
# =============================================================================
#
# Usage:
#   scripts/session/integrate.sh                    # Interactive mode
#   scripts/session/integrate.sh --auto             # Auto-merge all ready branches
#   scripts/session/integrate.sh --branch campaign/narrowing  # Merge specific branch
#   scripts/session/integrate.sh --dry-run          # Show what would be merged
#
# For each campaign branch with commits ahead of main:
#   1. Creates a temp merge onto latest main
#   2. Builds and runs targeted conformance tests
#   3. If no regression: fast-forward merges to main and pushes
#   4. If regression: reports failure, skips branch
#
# Designed to be run by the integrator agent via:
#   /loop 30m run scripts/session/integrate.sh --auto
#
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"

AUTO=false
DRY_RUN=false
SPECIFIC_BRANCH=""

# Cleanup temp worktrees on exit/interrupt
TEMP_DIRS=()
cleanup_temps() {
    for dir in "${TEMP_DIRS[@]+"${TEMP_DIRS[@]}"}"; do
        [[ -z "$dir" ]] && continue
        git -C "$REPO_ROOT" worktree remove "$dir" --force 2>/dev/null || rm -rf "$dir"
    done
    # Clean up any temp merge branches
    git -C "$REPO_ROOT" branch --list 'merge-validation-*' 2>/dev/null | while read -r b; do
        git -C "$REPO_ROOT" branch -D "$b" 2>/dev/null || true
    done
}
trap cleanup_temps EXIT INT TERM

while [[ $# -gt 0 ]]; do
    case "$1" in
        --auto) AUTO=true; shift ;;
        --dry-run) DRY_RUN=true; shift ;;
        --branch) SPECIFIC_BRANCH="$2"; shift 2 ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

# --- Fetch latest ---
echo "Fetching latest from origin..."
git -C "$REPO_ROOT" fetch origin --quiet

# --- Get current main conformance baseline ---
BASELINE_PASS=0
if [[ -f "$REPO_ROOT/scripts/conformance/conformance-snapshot.json" ]]; then
    BASELINE_PASS=$(python3 -c "
import json
with open('$REPO_ROOT/scripts/conformance/conformance-snapshot.json') as f:
    print(json.load(f).get('summary', {}).get('passed', 0))
" 2>/dev/null || echo "0")
fi
echo "Current main conformance baseline: $BASELINE_PASS tests passing"

# --- Find campaign branches with new commits ---
if [[ -n "$SPECIFIC_BRANCH" ]]; then
    branches=("$SPECIFIC_BRANCH")
else
    branches=()
    while IFS= read -r b; do
        branches+=("$b")
    done < <(git -C "$REPO_ROOT" branch -r 2>/dev/null | grep "origin/campaign/" | sed 's|origin/||' | tr -d ' ')
fi

if [[ ${#branches[@]} -eq 0 ]]; then
    echo "No campaign branches found."
    exit 0
fi

echo ""
echo "Campaign branches found:"
for branch in "${branches[@]}"; do
    ahead=$(git -C "$REPO_ROOT" rev-list --count "origin/main..origin/$branch" 2>/dev/null || echo "0")
    if [[ "$ahead" -gt 0 ]]; then
        echo "  $branch ($ahead commits ahead)"
    fi
done

# --- Process each branch ---
MERGED=0
FAILED=0
SKIPPED=0

for branch in "${branches[@]}"; do
    ahead=$(git -C "$REPO_ROOT" rev-list --count "origin/main..origin/$branch" 2>/dev/null || echo "0")

    if [[ "$ahead" -eq 0 ]]; then
        continue
    fi

    echo ""
    echo "============================================="
    echo "Processing: $branch ($ahead commits ahead)"
    echo "============================================="

    # Show commits
    echo "Commits:"
    git -C "$REPO_ROOT" log --oneline "origin/main..origin/$branch" | head -10

    if $DRY_RUN; then
        echo "[DRY RUN] Would attempt merge and validation"
        continue
    fi

    if ! $AUTO; then
        read -p "Attempt merge and validate? [y/N] " confirm
        if [[ "$confirm" != "y" ]]; then
            echo "Skipped."
            ((SKIPPED++))
            continue
        fi
    fi

    # Record main SHA before validation (to detect race conditions later)
    VALIDATED_MAIN=$(git -C "$REPO_ROOT" rev-parse origin/main)

    # Create temp worktree for merge validation
    MERGE_DIR=$(mktemp -d "${TMPDIR:-/tmp}/tsz-merge-XXXXXX")
    TEMP_DIRS+=("$MERGE_DIR")
    echo "Creating merge validation worktree at $MERGE_DIR..."

    # Create a temp branch for the merge
    MERGE_BRANCH="merge-validation-$(date +%s)"
    git -C "$REPO_ROOT" worktree add "$MERGE_DIR" -b "$MERGE_BRANCH" origin/main --quiet 2>/dev/null

    # Attempt merge
    if ! git -C "$MERGE_DIR" merge "origin/$branch" --no-edit --quiet 2>/dev/null; then
        echo "MERGE CONFLICT — cannot auto-merge $branch"
        echo "  Manual resolution needed. Skipping."
        git -C "$REPO_ROOT" worktree remove "$MERGE_DIR" --force 2>/dev/null || rm -rf "$MERGE_DIR"
        git -C "$REPO_ROOT" branch -D "$MERGE_BRANCH" 2>/dev/null || true
        ((FAILED++))
        continue
    fi

    echo "Merge succeeded. Running validation..."

    # Build check
    echo "  Building..."
    if ! cargo check --manifest-path "$MERGE_DIR/Cargo.toml" 2>&1 | tail -3; then
        echo "BUILD FAILED — rejecting $branch"
        git -C "$REPO_ROOT" worktree remove "$MERGE_DIR" --force 2>/dev/null || rm -rf "$MERGE_DIR"
        git -C "$REPO_ROOT" branch -D "$MERGE_BRANCH" 2>/dev/null || true
        ((FAILED++))
        continue
    fi

    # Conformance check
    echo "  Running conformance tests..."
    # Strip ANSI color codes before parsing conformance output
    MERGE_PASS=$(cd "$MERGE_DIR" && scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' | grep -Eo '[0-9]+/[0-9]+ passed' | grep -Eo '^[0-9]+' || echo "0")

    if [[ "$MERGE_PASS" -lt "$BASELINE_PASS" ]]; then
        REGRESSION=$((BASELINE_PASS - MERGE_PASS))
        echo "REGRESSION — $branch loses $REGRESSION tests ($MERGE_PASS vs $BASELINE_PASS baseline)"
        echo "  Rejecting merge. Campaign agent needs to investigate."
        git -C "$REPO_ROOT" worktree remove "$MERGE_DIR" --force 2>/dev/null || rm -rf "$MERGE_DIR"
        git -C "$REPO_ROOT" branch -D "$MERGE_BRANCH" 2>/dev/null || true
        ((FAILED++))
        continue
    fi

    IMPROVEMENT=$((MERGE_PASS - BASELINE_PASS))
    echo "  Conformance: $MERGE_PASS tests passing (+$IMPROVEMENT from baseline)"

    # Clean up temp worktree
    git -C "$REPO_ROOT" worktree remove "$MERGE_DIR" --force 2>/dev/null || rm -rf "$MERGE_DIR"
    git -C "$REPO_ROOT" branch -D "$MERGE_BRANCH" 2>/dev/null || true

    # Actually merge to main
    echo "  Merging to main..."

    # Re-fetch to check for race condition (main may have advanced during validation)
    git -C "$REPO_ROOT" fetch origin main --quiet
    CURRENT_MAIN=$(git -C "$REPO_ROOT" rev-parse origin/main)
    if [[ "$CURRENT_MAIN" != "$VALIDATED_MAIN" ]]; then
        echo "  WARNING: origin/main advanced during validation. Re-validating would be needed."
        echo "  Skipping this branch for now. Will retry on next integration cycle."
        ((SKIPPED++))
        continue
    fi

    # Checkout main, merge, push
    MAIN_DIR=$(mktemp -d "${TMPDIR:-/tmp}/tsz-main-XXXXXX")
    TEMP_DIRS+=("$MAIN_DIR")
    git -C "$REPO_ROOT" worktree add "$MAIN_DIR" --detach origin/main --quiet
    git -C "$MAIN_DIR" checkout -B main origin/main --quiet

    git -C "$MAIN_DIR" merge "origin/$branch" --no-edit --quiet
    git -C "$MAIN_DIR" push origin main --quiet

    echo "  Pushed to main. Conformance: $MERGE_PASS (+$IMPROVEMENT)"
    BASELINE_PASS=$MERGE_PASS

    # Clean up
    git -C "$REPO_ROOT" worktree remove "$MAIN_DIR" --force 2>/dev/null || rm -rf "$MAIN_DIR"

    # Delete the campaign branch from remote
    if $AUTO; then
        echo "  Deleting merged branch origin/$branch..."
        git -C "$REPO_ROOT" push origin --delete "${branch}" --quiet 2>/dev/null || true
    fi

    ((MERGED++))
done

echo ""
echo "============================================="
echo "Integration Summary"
echo "  Merged:  $MERGED"
echo "  Failed:  $FAILED"
echo "  Skipped: $SKIPPED"
echo "  New baseline: $BASELINE_PASS tests passing"
echo "============================================="
