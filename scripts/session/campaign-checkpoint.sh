#!/usr/bin/env bash
# =============================================================================
# campaign-checkpoint.sh — Record session progress and enforce exit gates
# =============================================================================
#
# Usage:
#   scripts/session/campaign-checkpoint.sh <campaign-name>              # Record progress
#   scripts/session/campaign-checkpoint.sh <campaign-name> --status     # Show progress history
#   scripts/session/campaign-checkpoint.sh <campaign-name> --init       # Initialize progress file
#
# Agents MUST run this before claiming a campaign session is done.
# It records the conformance delta and blocks premature "complete" claims.
#
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
PROGRESS_DIR="$SCRIPT_DIR/progress"

CAMPAIGN="${1:-}"
ACTION="${2:-checkpoint}"

if [[ -z "$CAMPAIGN" ]]; then
    echo "Usage: $0 <campaign-name> [--status|--init]"
    echo ""
    echo "Existing progress files:"
    ls "$PROGRESS_DIR"/*.json 2>/dev/null | xargs -I{} basename {} .json | sed 's/^/  /' || echo "  (none)"
    exit 1
fi

PROGRESS_FILE="$PROGRESS_DIR/${CAMPAIGN}.json"

# --- Initialize progress file ---
init_progress() {
    if [[ -f "$PROGRESS_FILE" ]]; then
        echo "Progress file already exists: $PROGRESS_FILE"
        return
    fi

    # Get current conformance baseline
    local baseline
    baseline=$(python3 -c "
import json
with open('$REPO_ROOT/scripts/conformance/conformance-snapshot.json') as f:
    print(json.load(f).get('summary', {}).get('passed', 0))
" 2>/dev/null || echo "0")

    python3 -c "
import json
data = {
    'campaign': '$CAMPAIGN',
    'baseline_at_creation': $baseline,
    'status': 'active',
    'sessions': [],
    'known_dead_ends': [],
    'promising_leads': [],
    'cross_cutting_blockers': []
}
with open('$PROGRESS_FILE', 'w') as f:
    json.dump(data, f, indent=2)
print('Initialized progress file for $CAMPAIGN (baseline: $baseline)')
"
}

# --- Show progress history ---
show_status() {
    if [[ ! -f "$PROGRESS_FILE" ]]; then
        echo "No progress file for '$CAMPAIGN'. Run with --init first."
        exit 1
    fi

    python3 -c "
import json
with open('$PROGRESS_FILE') as f:
    data = json.load(f)

print(f\"Campaign: {data['campaign']}\")
print(f\"Status:   {data['status']}\")
print(f\"Baseline: {data['baseline_at_creation']}\")
print()

sessions = data.get('sessions', [])
if sessions:
    print(f'Sessions ({len(sessions)}):')
    total_delta = 0
    for s in sessions:
        delta = s.get('delta', 0)
        total_delta += delta
        blocked = f\" | blocked: {s['blocked_on']}\" if s.get('blocked_on') else ''
        print(f\"  {s['date']}: +{delta} tests{blocked}\")
    print(f'  Total improvement: +{total_delta}')
else:
    print('No sessions recorded yet.')

print()

dead_ends = data.get('known_dead_ends', [])
if dead_ends:
    print(f'Known dead ends ({len(dead_ends)}):')
    for d in dead_ends:
        print(f'  - {d}')
    print()

leads = data.get('promising_leads', [])
if leads:
    print(f'Promising leads ({len(leads)}):')
    for l in leads:
        print(f'  - {l}')
    print()

blockers = data.get('cross_cutting_blockers', [])
if blockers:
    print(f'Cross-cutting blockers ({len(blockers)}):')
    for b in blockers:
        print(f'  - {b}')
"
}

# --- Record checkpoint ---
record_checkpoint() {
    if [[ ! -f "$PROGRESS_FILE" ]]; then
        echo "No progress file for '$CAMPAIGN'. Initializing..."
        init_progress
    fi

    # Get current conformance numbers
    echo "Running conformance to measure progress..."
    local current_pass
    current_pass=$(cd "$REPO_ROOT" && scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>/dev/null \
        | sed 's/\x1b\[[0-9;]*m//g' \
        | grep -Eo '[0-9]+/[0-9]+ passed' \
        | grep -Eo '^[0-9]+' || echo "0")

    # Get last session's end value (or baseline)
    local prev_pass
    prev_pass=$(python3 -c "
import json
with open('$PROGRESS_FILE') as f:
    data = json.load(f)
sessions = data.get('sessions', [])
if sessions:
    print(sessions[-1].get('end_pass', data['baseline_at_creation']))
else:
    print(data['baseline_at_creation'])
" 2>/dev/null || echo "0")

    local delta=$((current_pass - prev_pass))

    echo ""
    echo "============================================="
    echo "Campaign Checkpoint: $CAMPAIGN"
    echo "  Previous:  $prev_pass tests passing"
    echo "  Current:   $current_pass tests passing"
    echo "  Delta:     +$delta"
    echo "============================================="

    # Collect session metadata
    local branch_commits
    branch_commits=$(git -C "$REPO_ROOT" log --oneline "origin/main..HEAD" 2>/dev/null | head -20 || echo "(none)")
    local commit_count
    commit_count=$(git -C "$REPO_ROOT" rev-list --count "origin/main..HEAD" 2>/dev/null || echo "0")
    local today
    today=$(date -I)

    # Prompt for session notes (non-interactive: read from env vars)
    local blocked_on="${CHECKPOINT_BLOCKED_ON:-}"
    local tried_and_failed="${CHECKPOINT_TRIED:-}"
    local promising="${CHECKPOINT_LEADS:-}"

    if [[ $delta -lt 3 ]] && [[ -z "$blocked_on" ]]; then
        echo ""
        echo "WARNING: Only +$delta tests improved this session."
        echo ""
        echo "To record why, set these environment variables before running:"
        echo "  export CHECKPOINT_BLOCKED_ON='description of what blocked progress'"
        echo "  export CHECKPOINT_TRIED='approaches that were tried and failed'"
        echo "  export CHECKPOINT_LEADS='promising leads for next session'"
        echo ""
        echo "Or pass them inline:"
        echo "  CHECKPOINT_BLOCKED_ON='root cause is in solver narrowing' $0 $CAMPAIGN"
        echo ""
        echo "Recording session with low delta anyway..."
    fi

    # Update progress file
    python3 -c "
import json

with open('$PROGRESS_FILE') as f:
    data = json.load(f)

session = {
    'date': '$today',
    'start_pass': $prev_pass,
    'end_pass': $current_pass,
    'delta': $delta,
    'commits': $commit_count,
}

blocked = '''$blocked_on'''.strip()
if blocked:
    session['blocked_on'] = blocked

tried = '''$tried_and_failed'''.strip()
if tried:
    data.setdefault('known_dead_ends', []).append(tried)

leads = '''$promising'''.strip()
if leads:
    data['promising_leads'] = [l.strip() for l in leads.split(';') if l.strip()]

data.setdefault('sessions', []).append(session)

# Auto-set status based on cumulative progress
total_delta = sum(s.get('delta', 0) for s in data['sessions'])
num_sessions = len(data['sessions'])
low_progress_sessions = sum(1 for s in data['sessions'][-3:] if s.get('delta', 0) < 3)

if low_progress_sessions >= 3:
    data['status'] = 'diminishing'
elif blocked:
    data['status'] = 'blocked'
else:
    data['status'] = 'active'

with open('$PROGRESS_FILE', 'w') as f:
    json.dump(data, f, indent=2)

print()
print(f\"Status: {data['status']}\")
print(f\"Total improvement across {num_sessions} session(s): +{total_delta}\")
if data['status'] == 'diminishing':
    print('  -> Last 3 sessions had <3 test improvement each.')
    print('  -> Consider switching approach or campaign.')
if data.get('promising_leads'):
    print(f\"Promising leads for next session:\")
    for l in data['promising_leads']:
        print(f'  - {l}')
"
}

# --- Dispatch ---
case "$ACTION" in
    --init)     init_progress ;;
    --status)   show_status ;;
    checkpoint) record_checkpoint ;;
    *)          echo "Unknown action: $ACTION"; exit 1 ;;
esac
