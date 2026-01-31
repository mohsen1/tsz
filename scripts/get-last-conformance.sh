#!/bin/bash
#
# Get the last recorded conformance pass rate from git history
#
# Scans commit messages for "CURRENT_CONFORMANCE_PASS_RATE: XX.X%" and returns the most recent value.
# Searches up to 100 commits by default.
#
# Usage:
#   ./scripts/get-last-conformance.sh          # Get last conformance %
#   ./scripts/get-last-conformance.sh --sha    # Also print the commit SHA
#
# Exit codes:
#   0 - Found conformance value (printed to stdout)
#   1 - No conformance value found in history

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

PRINT_SHA=false
MAX_COMMITS=100

while [[ $# -gt 0 ]]; do
    case $1 in
        --sha) PRINT_SHA=true ;;
        --max=*) MAX_COMMITS="${1#*=}" ;;
        *) ;;
    esac
    shift
done

cd "$ROOT_DIR"

# Search through recent commits for conformance pass rate
# Format expected: "CURRENT_CONFORMANCE_PASS_RATE: XX.X%"
while IFS= read -r line; do
    SHA=$(echo "$line" | cut -d' ' -f1)
    
    # Get the full commit message
    MESSAGE=$(git log -1 --format=%B "$SHA" 2>/dev/null || continue)
    
    # Look for the specific pattern: "CURRENT_CONFORMANCE_PASS_RATE: XX.X%"
    CONFORMANCE=$(echo "$MESSAGE" | grep -oE 'CURRENT_CONFORMANCE_PASS_RATE:\s*[0-9]+(\.[0-9]+)?%' | head -1 | grep -oE '[0-9]+(\.[0-9]+)?')
    
    if [[ -n "$CONFORMANCE" ]]; then
        if [[ "$PRINT_SHA" == true ]]; then
            echo "$CONFORMANCE $SHA"
        else
            echo "$CONFORMANCE"
        fi
        exit 0
    fi
done < <(git log --oneline -n "$MAX_COMMITS" 2>/dev/null)

# No conformance found
exit 1
