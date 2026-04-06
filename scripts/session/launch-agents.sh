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

# --- Select campaigns using shared logic ---
# shellcheck source=_select-campaigns.sh
source "$SCRIPT_DIR/_select-campaigns.sh"

mapfile -t available < <(_select_campaigns)

echo ""
echo "============================================="
echo "Launching ${#available[@]} agents (max: $MAX_AGENTS, stagger: ${STAGGER_SECONDS}s)"
echo "============================================="
for c in "${available[@]}"; do
    echo "  - $c"
done
echo ""

if [[ ${#available[@]} -eq 0 ]]; then
    echo "No campaigns available to launch."
    exit 0
fi

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
    echo "  Start OpenCode in that directory:"
    echo "    cd .worktrees/$campaign && opencode -m alibaba/qwen3.6-plus"
    echo ""

    LAUNCHED=$((LAUNCHED + 1))
done

echo "============================================="
echo "Launched $LAUNCHED agents."
echo "Start OpenCode in each worktree directory, or use:"
echo "  scripts/session/launch-agents-opencode.sh  # headless mode"
echo "============================================="
