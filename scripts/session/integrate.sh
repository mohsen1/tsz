#!/usr/bin/env bash
# =============================================================================
# integrate.sh — Validate campaign branches and open pull requests
# =============================================================================
#
# Usage:
#   scripts/session/integrate.sh                    # Interactive mode
#   scripts/session/integrate.sh --auto             # Auto-open PRs for all ready branches
#   scripts/session/integrate.sh --branch campaign/narrowing  # Process a specific branch
#   scripts/session/integrate.sh --dry-run          # Show what would be processed
#
# For each campaign branch with commits ahead of main:
#   1. Creates a temp merge onto latest main (validation only — never pushed)
#   2. Builds and runs targeted conformance tests
#   3. If no regression: opens (or updates) a pull request targeting main
#   4. If regression: reports failure, skips branch
#
# This script NEVER pushes directly to main. Merging into main is done via the
# pull-request review flow on GitHub, not by this script.
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
    git -C "$REPO_ROOT" log --oneline -10 "origin/main..origin/$branch"

    if $DRY_RUN; then
        echo "[DRY RUN] Would attempt merge and validation"
        continue
    fi

    if ! $AUTO; then
        read -p "Attempt merge and validate? [y/N] " confirm
        if [[ "$confirm" != "y" ]]; then
            echo "Skipped."
            SKIPPED=$((SKIPPED + 1))
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

    # Initialize submodules in worktree (needed for TypeScript test fixtures)
    git -C "$MERGE_DIR" submodule update --init --quiet 2>/dev/null

    # Attempt merge
    if ! git -C "$MERGE_DIR" merge "origin/$branch" --no-edit --quiet 2>/dev/null; then
        echo "MERGE CONFLICT — cannot auto-merge $branch"
        echo "  Manual resolution needed. Skipping."
        git -C "$REPO_ROOT" worktree remove "$MERGE_DIR" --force 2>/dev/null || rm -rf "$MERGE_DIR"
        git -C "$REPO_ROOT" branch -D "$MERGE_BRANCH" 2>/dev/null || true
        FAILED=$((FAILED + 1))
        continue
    fi

    echo "Merge succeeded. Running validation..."

    # Unset CARGO_TARGET_DIR so the worktree's .cargo/config.toml target-dir = ".target" takes effect.
    # Otherwise builds go to the shared cache and the conformance script can't find the binary.
    unset CARGO_TARGET_DIR

    # Build check
    echo "  Building..."
    if ! cargo check --manifest-path "$MERGE_DIR/Cargo.toml" 2>&1 | tail -3; then
        echo "BUILD FAILED — rejecting $branch"
        git -C "$REPO_ROOT" worktree remove "$MERGE_DIR" --force 2>/dev/null || rm -rf "$MERGE_DIR"
        git -C "$REPO_ROOT" branch -D "$MERGE_BRANCH" 2>/dev/null || true
        FAILED=$((FAILED + 1))
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
        FAILED=$((FAILED + 1))
        continue
    fi

    IMPROVEMENT=$((MERGE_PASS - BASELINE_PASS))
    echo "  Conformance: $MERGE_PASS tests passing (+$IMPROVEMENT from baseline)"

    # Clean up temp worktree
    git -C "$REPO_ROOT" worktree remove "$MERGE_DIR" --force 2>/dev/null || rm -rf "$MERGE_DIR"
    git -C "$REPO_ROOT" branch -D "$MERGE_BRANCH" 2>/dev/null || true

    # Open (or update) a pull request against main instead of pushing directly.
    echo "  Opening pull request for $branch..."

    PR_TITLE="integrate: $branch (+$IMPROVEMENT tests)"
    PR_BODY=$(printf "Validated by scripts/session/integrate.sh.\n\nConformance: %s tests passing (+%s vs baseline %s).\n\nMerge this PR via the GitHub review flow; integrate.sh never pushes to main directly." \
        "$MERGE_PASS" "$IMPROVEMENT" "$BASELINE_PASS")

    if command -v gh >/dev/null 2>&1; then
        if gh pr view "$branch" >/dev/null 2>&1; then
            echo "  PR already exists for $branch; leaving it for reviewer."
        else
            gh pr create \
                --base main \
                --head "$branch" \
                --title "$PR_TITLE" \
                --body "$PR_BODY" \
                >/dev/null || {
                    echo "  Failed to open PR via gh. Open one manually for $branch."
                    FAILED=$((FAILED + 1))
                    continue
                }
            echo "  PR opened for $branch."
        fi
    else
        echo "  gh CLI not installed. Please open a PR manually:"
        echo "    base: main"
        echo "    head: $branch"
        echo "    title: $PR_TITLE"
    fi

    MERGED=$((MERGED + 1))
done

echo ""
echo "============================================="
echo "Integration Summary"
echo "  PRs opened/updated: $MERGED"
echo "  Failed:             $FAILED"
echo "  Skipped:            $SKIPPED"
echo "  Validated baseline: $BASELINE_PASS tests passing"
echo "  (Merging into main happens via the GitHub PR review flow.)"
echo "============================================="
