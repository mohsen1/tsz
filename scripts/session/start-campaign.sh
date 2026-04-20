#!/usr/bin/env bash
# =============================================================================
# start-campaign.sh — Create an isolated worktree for a campaign session
# =============================================================================
#
# Usage: scripts/session/start-campaign.sh <campaign-name>
#
# Creates a worktree at .worktrees/<campaign> on branch campaign/<campaign>
# branched from origin/main. Checks if the campaign branch already exists.
#
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
CAMPAIGNS_FILE="$SCRIPT_DIR/campaigns.yaml"

# --- Parse arguments ---
CAMPAIGN="${1:-}"
if [[ -z "$CAMPAIGN" ]]; then
    echo "Usage: $0 <campaign-name>"
    echo ""
    echo "Available campaigns:"
    # Parse campaign names from YAML (simple grep, no yq dependency)
    grep -E '^  [a-z]' "$CAMPAIGNS_FILE" | sed 's/://' | while read -r name; do
        desc=$(grep -A1 "^  ${name}:" "$CAMPAIGNS_FILE" | grep 'description:' | sed 's/.*description: *>//' | sed 's/.*description: *//' | head -1)
        printf "  %-25s %s\n" "$name" "$desc"
    done
    exit 1
fi

BRANCH="campaign/$CAMPAIGN"
WORKTREE_DIR="$REPO_ROOT/.worktrees/$CAMPAIGN"

# --- Validate campaign exists ---
if ! grep -qE "^  ${CAMPAIGN}:" "$CAMPAIGNS_FILE"; then
    echo "ERROR: Unknown campaign '$CAMPAIGN'"
    echo "Run '$0' without arguments to see available campaigns."
    exit 1
fi

# --- Check if worktree already exists locally ---
if [[ -d "$WORKTREE_DIR" ]]; then
    echo "Worktree already exists at $WORKTREE_DIR"
    echo "To use it: cd $WORKTREE_DIR"
    exit 0
fi

# --- Fetch latest remote state ---
echo "Fetching latest from origin..."
git -C "$REPO_ROOT" fetch origin --quiet
"$REPO_ROOT/scripts/setup/install-hooks.sh"

# --- Check if campaign branch already exists on remote ---
if git -C "$REPO_ROOT" rev-parse --verify "origin/$BRANCH" &>/dev/null; then
    echo ""
    echo "WARNING: Branch '$BRANCH' already exists on origin."
    echo "  Another session may already be using this campaign branch."
    echo ""
    echo "Options:"
    echo "  1) Continue anyway (collaborate on same campaign branch)"
    echo "  2) Create a variant: campaign/${CAMPAIGN}-$(hostname -s)"
    echo "  3) Abort and pick a different campaign"
    echo ""
    read -p "Choice [1/2/3]: " choice
    case "$choice" in
        1)
            echo "Checking out existing branch..."
            git -C "$REPO_ROOT" worktree add "$WORKTREE_DIR" "$BRANCH"
            ;;
        2)
            VARIANT="campaign/${CAMPAIGN}-$(hostname -s)"
            echo "Creating variant branch: $VARIANT"
            git -C "$REPO_ROOT" worktree add "$WORKTREE_DIR" -b "$VARIANT" origin/main
            BRANCH="$VARIANT"
            ;;
        *)
            echo "Aborted."
            exit 0
            ;;
    esac
else
    # --- Ensure .worktrees is gitignored ---
    if ! git -C "$REPO_ROOT" check-ignore -q .worktrees 2>/dev/null; then
        echo ".worktrees" >> "$REPO_ROOT/.gitignore"
        echo "Added .worktrees to .gitignore"
    fi

    # --- Create worktree on new branch ---
    echo "Creating worktree at $WORKTREE_DIR on branch $BRANCH..."
    git -C "$REPO_ROOT" worktree add "$WORKTREE_DIR" -b "$BRANCH" origin/main
fi

# --- Configure shared CARGO_TARGET_DIR if not set ---
SHARED_TARGET="${CARGO_TARGET_DIR:-}"
if [[ -z "$SHARED_TARGET" ]]; then
    SHARED_TARGET="$HOME/.cache/tsz-target"
    echo ""
    echo "TIP: Set CARGO_TARGET_DIR to share build cache across worktrees:"
    echo "  export CARGO_TARGET_DIR=$SHARED_TARGET"
    echo ""
    echo "Add to your shell profile to persist. Saves ~5GB per worktree."
fi

# --- Print campaign info ---
echo ""
echo "============================================="
echo "Campaign: $CAMPAIGN"
echo "Branch:   $BRANCH"
echo "Worktree: $WORKTREE_DIR"
echo "============================================="
echo ""
echo "Next steps:"
echo "  cd $WORKTREE_DIR"
echo ""
echo "Open a Claude or Codex session rooted at:"
echo "  $WORKTREE_DIR"
echo ""
echo "Suggested kickoff prompt:"
echo "  Start the $CAMPAIGN campaign. Read scripts/session/AGENT_PROTOCOL.md and scripts/session/campaigns.yaml. Then run the health check, KPI dashboard, and checkpoint status before coding."
echo ""
echo "That session should:"
echo "  1. Read scripts/session/AGENT_PROTOCOL.md and campaigns.yaml"
echo "  2. Research failures with query-conformance.py --dashboard"
echo "  3. Plan the fix and verify against code"
echo "  4. Implement, then run scripts/session/verify-all.sh (ALL test suites)"
echo "  5. Record progress with campaign-checkpoint.sh before finishing"
echo ""
