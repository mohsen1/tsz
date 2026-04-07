#!/usr/bin/env bash
# =============================================================================
# launch-agents.sh — Simple campaign loop: work → validate → push → repeat
# =============================================================================
#
# Usage:
#   scripts/session/launch-agents.sh <campaign>     # Run campaign loop
#   scripts/session/launch-agents.sh --list         # List available campaigns
#
# Environment:
#   CLAUDE_MODEL    — model to use (default: claude-sonnet-4-20250514)
#   MAX_BUDGET      — per-iteration budget in USD (default: 5)
#
# =============================================================================
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
CAMPAIGNS_FILE="$SCRIPT_DIR/campaigns.yaml"
MODEL="${CLAUDE_MODEL:-claude-sonnet-4-20250514}"

# --- --list mode ---
if [[ "${1:-}" == "--list" ]]; then
    grep -E '^  [a-z0-9][a-z0-9-]*:$' "$CAMPAIGNS_FILE" | sed 's/://' | tr -d ' ' | grep -v '^integrator$'
    exit 0
fi

# --- Validate campaign argument ---
CAMPAIGN="${1:?Usage: $0 <campaign> | --list}"
if ! grep -qF "  ${CAMPAIGN}:" "$CAMPAIGNS_FILE"; then
    echo "Unknown campaign: $CAMPAIGN" >&2
    echo "Run '$0 --list' to see available campaigns." >&2
    exit 1
fi

WORKTREE="$REPO_ROOT/.worktrees/$CAMPAIGN"
BRANCH="campaign/$CAMPAIGN"

# --- Extract campaign section from YAML ---
extract_campaign() {
    awk -v name="  $CAMPAIGN:" '
        $0 == name { found=1; next }
        found && /^  [a-z0-9]/ { exit }
        found { print }
    ' "$CAMPAIGNS_FILE"
}

# --- Build agent prompt ---
build_prompt() {
    local campaign_section
    campaign_section="$(extract_campaign)"

    cat <<PROMPT
You are working on the tsz TypeScript compiler (Rust). Your campaign: "$CAMPAIGN"

## Campaign definition
$campaign_section

## Instructions
- Read .claude/CLAUDE.md for architecture rules. Solver owns WHAT, Checker owns WHERE.
- Research first: python3 scripts/conformance/query-conformance.py --campaign $CAMPAIGN
- Run verbose tests to understand failures before coding.
- Batch fixes that flip multiple tests. One root-cause fix > many one-off tweaks.
- Use cargo nextest run for unit tests. Use conformance query tools for research.
- Commit changes with descriptive messages referencing the campaign.

## CRITICAL SAFETY RULES
- Do NOT run git push. The automation validates and pushes for you.
- Do NOT break existing tests. Run cargo nextest run before committing.
- Do NOT run the full conformance suite for research — use the offline query tools.
- If unsure about a change, test it narrowly first with --filter.
PROMPT
}

# --- Setup worktree ---
setup_worktree() {
    if [[ -d "$WORKTREE" ]] && git -C "$WORKTREE" rev-parse --git-dir &>/dev/null; then
        echo "Reusing existing worktree: $WORKTREE"
        return 0
    fi
    if [[ -d "$WORKTREE" ]]; then
        echo "Removing broken worktree..."
        git -C "$REPO_ROOT" worktree remove "$WORKTREE" --force 2>/dev/null || rm -rf "$WORKTREE"
    fi
    echo "Creating worktree at $WORKTREE..."
    git -C "$REPO_ROOT" worktree add -B "$BRANCH" "$WORKTREE" origin/main
}

# --- Validate agent's work (runs in subshell to isolate cd) ---
validate() (
    local baseline="$2"
    cd "$1" || return 1

    echo "=== Validating: cargo fmt --check ==="
    if ! cargo fmt --check; then
        echo "FAIL: cargo fmt"
        return 1
    fi

    echo "=== Validating: cargo nextest run ==="
    if ! "$REPO_ROOT/scripts/safe-run.sh" cargo nextest run; then
        echo "FAIL: unit tests"
        return 1
    fi

    echo "=== Validating: conformance (baseline: $baseline) ==="
    local output pass_count
    output=$("$REPO_ROOT/scripts/safe-run.sh" ./scripts/conformance/conformance.sh run 2>/dev/null) || true
    pass_count=$(echo "$output" | sed 's/\x1b\[[0-9;]*m//g' | grep -Eo '[0-9]+/[0-9]+ passed' | grep -Eo '^[0-9]+' || echo "0")

    if ! [[ "$pass_count" =~ ^[0-9]+$ ]]; then
        echo "FAIL: could not parse conformance results"
        return 1
    fi

    if [[ "$pass_count" -lt "$baseline" ]]; then
        echo "FAIL: conformance regression ($pass_count < $baseline baseline)"
        return 1
    fi

    echo "OK: conformance $pass_count (baseline $baseline)"
)

# =============================================================================
# Main loop
# =============================================================================
setup_worktree
PROMPT="$(build_prompt)"

echo ""
echo "============================================="
echo "Campaign loop: $CAMPAIGN"
echo "Worktree: $WORKTREE"
echo "Model: $MODEL"
echo "============================================="
echo ""

ITERATION=0
while true; do
    ITERATION=$((ITERATION + 1))
    echo ""
    echo ">>> Iteration $ITERATION — $(date '+%H:%M:%S')"
    echo ""

    git -C "$WORKTREE" fetch origin main
    git -C "$WORKTREE" reset --hard origin/main
    CHECKPOINT=$(git -C "$WORKTREE" rev-parse HEAD)

    BASELINE=$(python3 -c "
import json, sys
with open(sys.argv[1]) as f:
    print(json.load(f).get('summary', {}).get('passed', 0))
" "$WORKTREE/scripts/conformance/conformance-snapshot.json" 2>/dev/null || echo "0")
    echo "Baseline: $BASELINE conformance tests passing"

    echo "Launching claude agent..."
    claude -p "$PROMPT" \
        --dir "$WORKTREE" \
        --dangerously-skip-permissions \
        --model "$MODEL" \
        || true

    CURRENT=$(git -C "$WORKTREE" rev-parse HEAD)
    if [[ "$CURRENT" == "$CHECKPOINT" ]]; then
        echo "No new commits. Sleeping 30s..."
        sleep 30
        continue
    fi

    COMMIT_COUNT=$(git -C "$WORKTREE" rev-list --count "$CHECKPOINT..$CURRENT")
    echo "Agent made $COMMIT_COUNT commit(s)."

    if validate "$WORKTREE" "$BASELINE"; then
        echo ""
        echo "Validation PASSED. Pushing to main..."
        if git -C "$WORKTREE" push origin HEAD:main; then
            echo "Pushed $COMMIT_COUNT commit(s) to main."
        else
            echo "Push failed (race condition?). Will retry next iteration."
        fi
    else
        echo ""
        echo "Validation FAILED. Resetting to checkpoint."
        git -C "$WORKTREE" reset --hard "$CHECKPOINT"
        # Back off — wait for main to advance before retrying
        echo "Waiting 60s before next iteration..."
        sleep 60
    fi
done
