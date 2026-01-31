#!/bin/bash
#
# Conformance check for pre-commit hook
#
# Runs the full conformance test suite and outputs only the pass rate.
#
# Usage:
#   ./scripts/quick-conformance.sh              # Run all tests, print percentage
#   ./scripts/quick-conformance.sh --json       # Output as JSON
#
# Output (default):
#   85.3
#
# Output (--json):
#   {"passRate": 85.3, "passed": 12345, "total": 14500}

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

JSON_OUTPUT=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --json) JSON_OUTPUT=true ;;
        *) ;;
    esac
    shift
done

cd "$ROOT_DIR"

# Create a temp file to capture output
TEMP_OUTPUT=$(mktemp)
trap "rm -f $TEMP_OUTPUT" EXIT

# Run ALL conformance tests
./conformance/run.sh 2>&1 | tee "$TEMP_OUTPUT"

# Extract pass rate from output
# Format: "Pass Rate: XX.X% (passed/total)"
PASS_LINE=$(grep -E "^Pass Rate:" "$TEMP_OUTPUT" | tail -1)

if [[ -z "$PASS_LINE" ]]; then
    echo "Error: Could not extract pass rate from conformance output" >&2
    exit 1
fi

# Parse the percentage and counts
PASS_RATE=$(echo "$PASS_LINE" | grep -oE '[0-9]+\.[0-9]+' | head -1)
PASSED=$(echo "$PASS_LINE" | grep -oE '\([0-9,]+/' | tr -d '(/' | tr -d ',')
TOTAL=$(echo "$PASS_LINE" | grep -oE '/[0-9,]+\)' | tr -d '/)' | tr -d ',')

if [[ "$JSON_OUTPUT" == true ]]; then
    echo "{\"passRate\": $PASS_RATE, \"passed\": $PASSED, \"total\": $TOTAL}"
else
    echo "$PASS_RATE"
fi
