#!/usr/bin/env bash
# =============================================================================
# check-status.sh — Show status of all campaigns and disk usage
# =============================================================================
#
# Usage: scripts/session/check-status.sh [--compact]
#
# Shows:
#   - All campaign branches (claimed vs available)
#   - Last commit age and message for active campaigns
#   - Disk usage for worktrees and target directories
#   - Current conformance baseline
#
# Designed to be run via /loop for periodic awareness.
#
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
CAMPAIGNS_FILE="$SCRIPT_DIR/campaigns.yaml"
COMPACT="${1:-}"

# --- Fetch latest remote state ---
git -C "$REPO_ROOT" fetch origin --quiet 2>/dev/null || true

echo "============================================="
echo "TSZ Campaign Status — $(date '+%Y-%m-%d %H:%M')"
echo "============================================="
echo ""

# --- Campaign status ---
echo "CAMPAIGNS:"
echo "----------"

# Get all defined campaigns
campaigns=$(grep -E '^  [a-z][a-z-]*:$' "$CAMPAIGNS_FILE" | sed 's/://' | tr -d ' ')

for campaign in $campaigns; do
    branch="campaign/$campaign"
    progress_file="$SCRIPT_DIR/progress/${campaign}.json"

    # Get progress status if available
    progress_info=""
    if [[ -f "$progress_file" ]]; then
        progress_info=$(python3 -c "
import json
with open('$progress_file') as f:
    data = json.load(f)
status = data.get('status', '?')
sessions = data.get('sessions', [])
total_delta = sum(s.get('delta', 0) for s in sessions)
num_sessions = len(sessions)
blocked = data.get('cross_cutting_blockers', [])
leads = data.get('promising_leads', [])
parts = [f'status={status}', f'+{total_delta} in {num_sessions}s']
if blocked:
    parts.append(f'{len(blocked)} blockers')
if leads:
    parts.append(f'{len(leads)} leads')
print(' | '.join(parts))
" 2>/dev/null || echo "")
    fi

    # Check if branch exists on remote
    if git -C "$REPO_ROOT" rev-parse --verify "origin/$branch" &>/dev/null 2>&1; then
        # Get last commit info
        last_msg=$(git -C "$REPO_ROOT" log -1 --format="%s" "origin/$branch" 2>/dev/null | head -c 60)
        last_age=$(git -C "$REPO_ROOT" log -1 --format="%ar" "origin/$branch" 2>/dev/null)
        commits_ahead=$(git -C "$REPO_ROOT" rev-list --count "origin/main..origin/$branch" 2>/dev/null || echo "?")

        printf "  %-25s CLAIMED  %s ahead | %s\n" "$campaign" "$commits_ahead" "$last_age"
        if [[ "$COMPACT" != "--compact" ]]; then
            printf "  %-25s          └─ %s\n" "" "$last_msg"
            if [[ -n "$progress_info" ]]; then
                printf "  %-25s          └─ progress: %s\n" "" "$progress_info"
            fi
        fi
    else
        # Check for local branch
        if git -C "$REPO_ROOT" rev-parse --verify "$branch" &>/dev/null 2>&1; then
            printf "  %-25s LOCAL    (not pushed yet)\n" "$campaign"
        else
            printf "  %-25s AVAILABLE\n" "$campaign"
        fi
        if [[ -n "$progress_info" ]] && [[ "$COMPACT" != "--compact" ]]; then
            printf "  %-25s          └─ progress: %s\n" "" "$progress_info"
        fi
    fi
done

# --- Check for variant/unknown campaign branches ---
# Build list of known campaign names
known_campaigns=$(grep -E '^  [a-z][a-z0-9_-]*:$' "$CAMPAIGNS_FILE" | sed 's/://' | tr -d ' ')

# Find campaign branches not matching any known campaign name
variant_branches=""
while IFS= read -r rb; do
    rb_clean=$(echo "$rb" | sed 's|origin/||' | tr -d ' ')
    campaign_part="${rb_clean#campaign/}"
    is_known=false
    for kc in $known_campaigns; do
        if [[ "$campaign_part" == "$kc" ]]; then
            is_known=true
            break
        fi
    done
    if ! $is_known; then
        variant_branches="$variant_branches $rb_clean"
    fi
done < <(git -C "$REPO_ROOT" branch -r 2>/dev/null | grep "origin/campaign/" || true)

if [[ -n "$variant_branches" ]]; then
    echo ""
    echo "VARIANT BRANCHES:"
    for vb in $variant_branches; do
        commits_ahead=$(git -C "$REPO_ROOT" rev-list --count "origin/main..origin/$vb" 2>/dev/null || echo "?")
        last_age=$(git -C "$REPO_ROOT" log -1 --format="%ar" "origin/$vb" 2>/dev/null)
        printf "  %-35s %s ahead | %s\n" "$vb" "$commits_ahead" "$last_age"
    done
fi

echo ""

# --- Main branch conformance ---
echo "MAIN BRANCH:"
echo "----------"
main_commit=$(git -C "$REPO_ROOT" log -1 --format="%h %s" origin/main 2>/dev/null)
echo "  Latest: $main_commit"

if [[ -f "$REPO_ROOT/scripts/conformance/conformance-snapshot.json" ]]; then
    pass_rate=$(python3 -c "
import json
with open('$REPO_ROOT/scripts/conformance/conformance-snapshot.json') as f:
    s = json.load(f)
    summary = s.get('summary', {})
    p = summary.get('passed', '?')
    t = summary.get('total', '?')
    pct = summary.get('pass_rate', '?')
    print(f'  Conformance: {p}/{t} ({pct})')
" 2>/dev/null || echo "  Conformance: (snapshot unavailable)")
    echo "$pass_rate"
fi

echo ""

# --- Disk usage ---
if [[ "$COMPACT" != "--compact" ]]; then
    echo "DISK USAGE:"
    echo "----------"

    # Main target dir
    if [[ -d "$REPO_ROOT/target" ]]; then
        main_size=$(du -sh "$REPO_ROOT/target" 2>/dev/null | cut -f1)
        echo "  Main target/:           $main_size"
    fi

    # Shared target dir
    if [[ -n "${CARGO_TARGET_DIR:-}" ]] && [[ -d "$CARGO_TARGET_DIR" ]]; then
        shared_size=$(du -sh "$CARGO_TARGET_DIR" 2>/dev/null | cut -f1)
        echo "  Shared CARGO_TARGET_DIR: $shared_size"
    fi

    # Worktree target dirs
    total_wt_size=0
    for wt in "$REPO_ROOT/.worktrees"/*/; do
        if [[ -d "${wt}target" ]]; then
            wt_name=$(basename "$wt")
            wt_size=$(du -sh "${wt}target" 2>/dev/null | cut -f1)
            printf "  .worktrees/%-15s %s\n" "${wt_name}/target:" "$wt_size"
        fi
    done 2>/dev/null

    # Total disk free
    disk_free=$(df -h "$REPO_ROOT" 2>/dev/null | tail -1 | awk '{print $4}')
    echo ""
    echo "  Disk free: $disk_free"

    echo ""
fi

echo "============================================="
