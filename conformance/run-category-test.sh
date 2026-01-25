#!/bin/bash
# Test a specific category of conformance tests

set -e

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
TSZ_BIN="$ROOT_DIR/target/release/tsz"
TESTS_DIR="$ROOT_DIR/TypeScript/tests/cases/conformance"
BASELINES_DIR="$ROOT_DIR/TypeScript/tests/baselines/reference"

CATEGORY=${1:-enums}
MAX_TESTS=${2:-100}
PASSED=0
FAILED=0
TOTAL=0

echo "Category Test Runner: $CATEGORY"
echo "=============================="

# Find test files in category
TESTS=$(find "$TESTS_DIR/$CATEGORY" -name "*.ts" -type f 2>/dev/null | head -n "$MAX_TESTS")

for TEST_FILE in $TESTS; do
    TOTAL=$((TOTAL + 1))
    BASENAME=$(basename "$TEST_FILE" .ts)

    # Get expected errors from baseline
    BASELINE_FILE="$BASELINES_DIR/$BASENAME.errors.txt"
    if [ -f "$BASELINE_FILE" ]; then
        EXPECTED_CODES=$(grep -oE 'TS[0-9]+' "$BASELINE_FILE" | sort -u | tr '\n' ' ')
    else
        EXPECTED_CODES=""
    fi

    # Run tsz
    cd "$ROOT_DIR"
    ACTUAL=$(timeout 10 "$TSZ_BIN" "$TEST_FILE" 2>&1 || true)
    ACTUAL_CODES=$(echo "$ACTUAL" | grep -oE 'TS[0-9]+' | sort -u | tr '\n' ' ')

    # Compare
    if [ "$EXPECTED_CODES" = "$ACTUAL_CODES" ]; then
        PASSED=$((PASSED + 1))
    else
        FAILED=$((FAILED + 1))
        echo "[FAIL] $BASENAME"
        echo "  Expected: $EXPECTED_CODES"
        echo "  Actual:   $ACTUAL_CODES"
    fi
done

echo ""
echo "=============================="
echo "Results: $PASSED/$TOTAL passed ($FAILED failed)"
if [ $TOTAL -gt 0 ]; then
    RATE=$((PASSED * 100 / TOTAL))
    echo "Pass rate: $RATE%"
fi
