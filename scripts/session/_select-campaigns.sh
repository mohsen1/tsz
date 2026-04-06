#!/usr/bin/env bash
# =============================================================================
# _select-campaigns.sh — Shared campaign selection logic
# =============================================================================
#
# Outputs available campaign names (one per line) to stdout.
# Sourced by launch-agents.sh and launch-agents-opencode.sh.
#
# Required variables (set by caller):
#   REPO_ROOT, CAMPAIGNS_FILE, PROGRESS_DIR, MAX_AGENTS
#
# Optional:
#   SPECIFIC_CAMPAIGNS — space-separated override list
#
# =============================================================================

_select_campaigns() {
    local available=()

    if [[ -n "${SPECIFIC_CAMPAIGNS:-}" ]]; then
        # shellcheck disable=SC2206
        available=($SPECIFIC_CAMPAIGNS)
    else
        local all_campaigns
        all_campaigns=$(grep -E '^  [a-z][a-z-]*:$' "$CAMPAIGNS_FILE" | sed 's/://' | tr -d ' ')

        for campaign in $all_campaigns; do
            [[ "$campaign" == "integrator" ]] && continue

            # Skip if already claimed on remote
            if git -C "$REPO_ROOT" rev-parse --verify "origin/campaign/$campaign" &>/dev/null 2>&1; then
                local ahead
                ahead=$(git -C "$REPO_ROOT" rev-list --count "origin/main..origin/campaign/$campaign" 2>/dev/null || echo "0")
                if [[ "$ahead" -gt 0 ]]; then
                    echo "Skipping $campaign — active on remote ($ahead commits ahead)" >&2
                    continue
                fi
            fi

            # Skip if progress file says diminishing
            local progress_file="$PROGRESS_DIR/${campaign}.json"
            if [[ -f "$progress_file" ]]; then
                local status
                status=$(python3 -c "
import json
with open('$progress_file') as f:
    print(json.load(f).get('status', 'active'))
" 2>/dev/null || echo "active")

                if [[ "$status" == "diminishing" ]]; then
                    echo "Skipping $campaign — status is 'diminishing'" >&2
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

    # Output to stdout
    for c in "${available[@]}"; do
        echo "$c"
    done
}
