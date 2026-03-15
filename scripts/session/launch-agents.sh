#!/usr/bin/env bash
# =============================================================================
# launch-agents.sh — Launch campaign agents with staggered starts
# =============================================================================
#
# Usage:
#   scripts/session/launch-agents.sh                    # Launch up to 3 agents
#   scripts/session/launch-agents.sh --max 5            # Launch up to 5 agents
#   scripts/session/launch-agents.sh --stagger 180      # 3 min between launches
#   scripts/session/launch-agents.sh --dry-run          # Show what would launch
#   scripts/session/launch-agents.sh --campaigns "narrowing false-positives"
#
# Prevents the "10 agents all hit rate limits" failure mode by:
#   - Capping concurrent agents (default: 3)
#   - Staggering launches (default: 2 minutes apart)
#   - Skipping campaigns with status "diminishing" in progress files
#   - Running health check before first launch
#
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
CAMPAIGNS_FILE="$SCRIPT_DIR/campaigns.yaml"
PROGRESS_DIR="$SCRIPT_DIR/progress"

MAX_AGENTS=3
STAGGER_SECONDS=120
DRY_RUN=false
SPECIFIC_CAMPAIGNS=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --max) MAX_AGENTS="$2"; shift 2 ;;
        --stagger) STAGGER_SECONDS="$2"; shift 2 ;;
        --dry-run) DRY_RUN=true; shift ;;
        --campaigns) SPECIFIC_CAMPAIGNS="$2"; shift 2 ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

# --- Health check ---
if ! $DRY_RUN; then
    echo "Running health check before launching agents..."
    if ! "$SCRIPT_DIR/healthcheck.sh"; then
        echo "Aborting launch — main is unhealthy."
        exit 1
    fi
    echo ""
fi

# --- Determine which campaigns to launch ---
if [[ -n "$SPECIFIC_CAMPAIGNS" ]]; then
    # shellcheck disable=SC2206
    available=($SPECIFIC_CAMPAIGNS)
else
    available=()
    all_campaigns=$(grep -E '^  [a-z][a-z-]*:$' "$CAMPAIGNS_FILE" | sed 's/://' | tr -d ' ')

    for campaign in $all_campaigns; do
        # Skip integrator and performance (special campaigns)
        [[ "$campaign" == "integrator" ]] && continue

        # Skip if already claimed on remote
        if git -C "$REPO_ROOT" rev-parse --verify "origin/campaign/$campaign" &>/dev/null 2>&1; then
            ahead=$(git -C "$REPO_ROOT" rev-list --count "origin/main..origin/campaign/$campaign" 2>/dev/null || echo "0")
            if [[ "$ahead" -gt 0 ]]; then
                echo "Skipping $campaign — active on remote ($ahead commits ahead)"
                continue
            fi
        fi

        # Skip if progress file says diminishing
        progress_file="$PROGRESS_DIR/${campaign}.json"
        if [[ -f "$progress_file" ]]; then
            status=$(python3 -c "
import json
with open('$progress_file') as f:
    print(json.load(f).get('status', 'active'))
" 2>/dev/null || echo "active")

            if [[ "$status" == "diminishing" ]]; then
                echo "Skipping $campaign — status is 'diminishing'"
                continue
            fi
        fi

        available+=("$campaign")
    done
fi

# Cap to max agents
if [[ ${#available[@]} -gt $MAX_AGENTS ]]; then
    available=("${available[@]:0:$MAX_AGENTS}")
fi

echo ""
echo "============================================="
echo "Launching ${#available[@]} agents (max: $MAX_AGENTS, stagger: ${STAGGER_SECONDS}s)"
echo "============================================="
for c in "${available[@]}"; do
    echo "  - $c"
done
echo ""

if $DRY_RUN; then
    echo "(dry run — no agents launched)"
    exit 0
fi

# --- Launch agents ---
LAUNCHED=0
for campaign in "${available[@]}"; do
    if [[ $LAUNCHED -gt 0 ]]; then
        echo "Waiting ${STAGGER_SECONDS}s before next launch..."
        sleep "$STAGGER_SECONDS"
    fi

    echo "Launching agent for: $campaign"

    # Initialize progress file if it doesn't exist
    if [[ ! -f "$PROGRESS_DIR/${campaign}.json" ]]; then
        "$SCRIPT_DIR/campaign-checkpoint.sh" "$campaign" --init
    fi

    # Create worktree if needed
    "$SCRIPT_DIR/start-campaign.sh" "$campaign" <<< "1" 2>/dev/null || true

    echo "  Worktree ready at .worktrees/$campaign"
    echo "  Start Claude Code in that directory to begin work."
    echo ""

    LAUNCHED=$((LAUNCHED + 1))
done

echo "============================================="
echo "Launched $LAUNCHED agents."
echo "Start Claude Code in each worktree directory."
echo "============================================="
