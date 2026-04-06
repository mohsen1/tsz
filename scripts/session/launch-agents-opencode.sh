#!/usr/bin/env bash
# =============================================================================
# launch-agents-opencode.sh — Launch campaign agents via OpenCode CLI
# =============================================================================
#
# Usage:
#   scripts/session/launch-agents-opencode.sh                     # Launch up to 3 agents
#   scripts/session/launch-agents-opencode.sh --max 5             # Launch up to 5 agents
#   scripts/session/launch-agents-opencode.sh --stagger 180       # 3 min between launches
#   scripts/session/launch-agents-opencode.sh --dry-run           # Show what would launch
#   scripts/session/launch-agents-opencode.sh --campaigns "narrowing-boundary big3-unification"
#   scripts/session/launch-agents-opencode.sh --interactive       # Print instructions instead of running headless
#
# Model: alibaba/qwen3.6-plus (default, configurable via --model)
#
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
CAMPAIGNS_FILE="$SCRIPT_DIR/campaigns.yaml"
PROGRESS_DIR="$SCRIPT_DIR/progress"
LOG_DIR="$SCRIPT_DIR/logs"
PID_FILE="$LOG_DIR/agent-pids.txt"

MAX_AGENTS=3
STAGGER_SECONDS=180
DRY_RUN=false
INTERACTIVE=false
SPECIFIC_CAMPAIGNS=""
MODEL="alibaba/qwen3.6-plus"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --max)
            [[ -z "${2:-}" ]] && { echo "Error: --max requires a number"; exit 1; }
            MAX_AGENTS="$2"; shift 2 ;;
        --stagger)
            [[ -z "${2:-}" ]] && { echo "Error: --stagger requires a number"; exit 1; }
            STAGGER_SECONDS="$2"; shift 2 ;;
        --dry-run) DRY_RUN=true; shift ;;
        --interactive) INTERACTIVE=true; shift ;;
        --campaigns)
            [[ -z "${2:-}" ]] && { echo "Error: --campaigns requires a list"; exit 1; }
            SPECIFIC_CAMPAIGNS="$2"; shift 2 ;;
        --model)
            [[ -z "${2:-}" ]] && { echo "Error: --model requires a value"; exit 1; }
            MODEL="$2"; shift 2 ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

# Check opencode is installed
if ! command -v opencode &>/dev/null && ! $INTERACTIVE; then
    echo "Error: 'opencode' not found in PATH. Install it first."
    exit 1
fi

mkdir -p "$LOG_DIR"

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

available=()
while IFS= read -r line; do
    available+=("$line")
done < <(_select_campaigns)

echo ""
echo "============================================="
echo "Launching ${#available[@]} agents (max: $MAX_AGENTS, stagger: ${STAGGER_SECONDS}s)"
echo "Model: $MODEL"
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

# --- Build campaign prompt ---
build_prompt() {
    local campaign="$1"
    cat <<EOF
You are working on the "$campaign" campaign for tsz.

Start by:
1. Running scripts/session/healthcheck.sh
2. Reading scripts/session/AGENT_PROTOCOL.md
3. Reading your campaign definition in scripts/session/campaigns.yaml (look for "$campaign:")
4. Running: python3 scripts/conformance/query-conformance.py --dashboard
5. Running: python3 scripts/conformance/query-conformance.py --campaign $campaign

Follow the discipline cycle:
- Research (30-40%): understand the failure patterns, run 8-15 tests with --verbose
- Plan (10-15%): write down the shared invariant before coding
- Implement (20-25%): fix the root cause, follow it across crate boundaries
- Verify: run scripts/session/verify-all.sh — ALL suites must pass, zero regressions
- Push: only after verify-all.sh passes, push to main

Remember:
- Match tsc behavior exactly
- Batch fixes that flip 5+ tests > one-off tweaks
- Solver owns WHAT (type semantics), Checker owns WHERE (source context)
- Don't push if any test suite regresses
- Clean up disk periodically: cargo clean -p tsz-cli; rm -rf /tmp/tsz-* /tmp/tmp.*
- If disk gets tight (<10GB free), run: scripts/session/cleanup.sh --auto
EOF
}

# --- Launch agents ---
> "$PID_FILE"
LAUNCHED=0

for campaign in "${available[@]}"; do
    if [[ $LAUNCHED -gt 0 ]]; then
        echo "Waiting ${STAGGER_SECONDS}s before next launch..."
        sleep "$STAGGER_SECONDS"
    fi

    echo "Launching agent for: $campaign"

    # Initialize progress file if it doesn't exist
    if [[ ! -f "$PROGRESS_DIR/${campaign}.json" ]]; then
        "$SCRIPT_DIR/campaign-checkpoint.sh" "$campaign" --init || true
    fi

    # Create worktree if needed
    WORKTREE_DIR="$REPO_ROOT/.worktrees/$campaign"
    if [[ ! -d "$WORKTREE_DIR" ]]; then
        "$SCRIPT_DIR/start-campaign.sh" "$campaign" <<< "1" || {
            echo "  WARNING: Failed to create worktree for $campaign, skipping."
            continue
        }
    fi

    if [[ ! -d "$WORKTREE_DIR" ]]; then
        echo "  WARNING: Worktree $WORKTREE_DIR does not exist, skipping."
        continue
    fi

    if $INTERACTIVE; then
        echo "  Worktree ready at $WORKTREE_DIR"
        echo "  Start OpenCode:"
        echo "    cd $WORKTREE_DIR"
        echo "    opencode -m $MODEL"
        echo ""
    else
        PROMPT="$(build_prompt "$campaign")"
        LOG_FILE="$LOG_DIR/${campaign}.log"

        echo "  Launching headless: opencode run ... --dir $WORKTREE_DIR"
        echo "  Log: $LOG_FILE"

        opencode run "$PROMPT" \
            -m "$MODEL" \
            --variant high \
            --dir "$WORKTREE_DIR" \
            --title "campaign-$campaign" \
            > "$LOG_FILE" 2>&1 &

        PID=$!
        echo "$PID $campaign" >> "$PID_FILE"
        echo "  PID: $PID"
        echo ""
    fi

    LAUNCHED=$((LAUNCHED + 1))
done

echo "============================================="
echo "Launched $LAUNCHED agents."
if ! $INTERACTIVE; then
    echo "PIDs written to: $PID_FILE"
    echo ""
    echo "Monitor agents:"
    echo "  tail -f $LOG_DIR/<campaign>.log"
    echo ""
    echo "Check status:"
    echo "  cat $PID_FILE"
    echo "  ps -p \$(awk '{print \$1}' $PID_FILE | tr '\\n' ',')"
fi
echo "============================================="
