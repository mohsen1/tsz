#!/bin/bash
#
# Update README with conformance test results
#
# Usage:
#   ./scripts/update-readme.sh              # Run tests and update README
#   ./scripts/update-readme.sh --commit     # Also commit and push
#   ./scripts/update-readme.sh --max=500    # Run only 500 tests (faster)
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Defaults
COMMIT=false
MAX_TESTS="--all"
WORKERS=8

# Parse arguments
for arg in "$@"; do
    case $arg in
        --commit) COMMIT=true ;;
        --max=*) MAX_TESTS="--max=${arg#*=}" ;;
        --workers=*) WORKERS="${arg#*=}" ;;
        --help|-h)
            echo "Update README with conformance test results"
            echo ""
            echo "Usage: ./scripts/update-readme.sh [options]"
            echo ""
            echo "Options:"
            echo "  --commit      Commit and push changes to git"
            echo "  --max=N       Run only N tests (default: all)"
            echo "  --workers=N   Number of workers (default: 8)"
            echo "  --help        Show this help"
            exit 0
            ;;
    esac
done

cd "$ROOT_DIR"

echo "============================================================"
echo "         Update README Conformance Progress"
echo "============================================================"
echo ""

# Run conformance tests (server mode)
echo "Running conformance tests..."
OUTPUT=$(./conformance/run.sh $MAX_TESTS --workers=$WORKERS 2>&1) || true
echo "$OUTPUT"

# Parse results (Pass Rate is at the end of output)
if echo "$OUTPUT" | grep -q "Pass Rate:"; then
    # Get the last Pass Rate line (it's at the end now)
    PASS_RATE=$(echo "$OUTPUT" | grep "Pass Rate:" | tail -1 | sed -E 's/.*Pass Rate:[[:space:]]*([0-9.]+)%.*/\1/')
    # Parse passed/total - handle comma-separated numbers
    PASSED=$(echo "$OUTPUT" | grep "Pass Rate:" | tail -1 | sed -E 's/.*\(([0-9,]+)\/([0-9,]+)\).*/\1/' | tr -d ',')
    TOTAL=$(echo "$OUTPUT" | grep "Pass Rate:" | tail -1 | sed -E 's/.*\(([0-9,]+)\/([0-9,]+)\).*/\2/' | tr -d ',')

    echo ""
    echo "Results: $PASSED/$TOTAL tests passed ($PASS_RATE%)"
else
    echo "Failed to parse conformance test output"
    exit 1
fi

# Get TypeScript version
TS_VERSION=$(node -e "const v = require('./conformance/typescript-versions.json'); const m = Object.values(v.mappings)[0]; console.log(m?.npm || v.default?.npm || 'unknown')")
echo "TypeScript version: $TS_VERSION"

# Update README
echo ""
echo "Updating README.md..."
cd conformance
npm run build --silent 2>/dev/null || npm run build
node dist/update-readme.js \
    --passed="$PASSED" \
    --total="$TOTAL" \
    --pass-rate="$PASS_RATE" \
    --ts-version="$TS_VERSION"

cd "$ROOT_DIR"

# Check if there are changes
if git diff --quiet README.md; then
    echo "README.md is already up to date"
else
    echo "README.md updated"

    if [ "$COMMIT" = true ]; then
        echo ""
        echo "Committing and pushing..."
        git add README.md
        git commit -m "docs: update conformance progress ($PASSED/$TOTAL tests passing)

Pass rate: $PASS_RATE%
TypeScript version: $TS_VERSION"
        git push
        echo "Pushed to remote"
    else
        echo ""
        echo "Run with --commit to commit and push changes"
        echo "Or manually: git add README.md && git commit -m 'docs: update conformance'"
    fi
fi

echo ""
echo "Done!"
