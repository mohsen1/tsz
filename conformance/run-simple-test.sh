#!/bin/bash
# Simple conformance test runner that handles multi-file tests

set -e

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
TSZ_BIN="$ROOT_DIR/target/release/tsz"
TESTS_DIR="$ROOT_DIR/TypeScript/tests/cases/conformance"
BASELINES_DIR="$ROOT_DIR/TypeScript/tests/baselines/reference"

MAX_TESTS=${1:-50}
PASSED=0
FAILED=0
TOTAL=0

echo "Simple Conformance Test Runner"
echo "=============================="
echo "TSZ binary: $TSZ_BIN"
echo "Max tests: $MAX_TESTS"
echo ""

# Find test files
TESTS=$(find "$TESTS_DIR" -name "*.ts" -type f | head -n "$MAX_TESTS")

for TEST_FILE in $TESTS; do
    TOTAL=$((TOTAL + 1))
    BASENAME=$(basename "$TEST_FILE" .ts)

    # Create temp directory
    TMP_DIR=$(mktemp -d)
    trap "rm -rf $TMP_DIR" EXIT

    # Get expected errors from baseline
    BASELINE_FILE="$BASELINES_DIR/$BASENAME.errors.txt"
    if [ -f "$BASELINE_FILE" ]; then
        EXPECTED_CODES=$(grep -oE 'TS[0-9]+' "$BASELINE_FILE" | sort -u | tr '\n' ' ')
    else
        EXPECTED_CODES=""
    fi

    # Check if multi-file test
    if grep -q '@filename:' "$TEST_FILE"; then
        # Multi-file test - split into individual files
        FILE_COUNT=0
        CURRENT_FILE=""
        CURRENT_CONTENT=""
        FILES_LIST=""

        while IFS= read -r line || [[ -n "$line" ]]; do
            if [[ "$line" =~ ^[[:space:]]*//@filename:[[:space:]]*(.+)$ ]]; then
                # Save previous file if exists
                if [ -n "$CURRENT_FILE" ]; then
                    mkdir -p "$(dirname "$TMP_DIR/$CURRENT_FILE")"
                    echo "$CURRENT_CONTENT" > "$TMP_DIR/$CURRENT_FILE"
                    FILES_LIST="$FILES_LIST $TMP_DIR/$CURRENT_FILE"
                    FILE_COUNT=$((FILE_COUNT + 1))
                fi
                CURRENT_FILE="${BASH_REMATCH[1]}"
                CURRENT_FILE=$(echo "$CURRENT_FILE" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
                CURRENT_CONTENT=""
            elif [[ "$line" =~ ^[[:space:]]*//@[a-zA-Z]+: ]]; then
                # Skip other directives
                :
            else
                if [ -n "$CURRENT_FILE" ]; then
                    CURRENT_CONTENT="$CURRENT_CONTENT
$line"
                fi
            fi
        done < "$TEST_FILE"

        # Save last file
        if [ -n "$CURRENT_FILE" ]; then
            mkdir -p "$(dirname "$TMP_DIR/$CURRENT_FILE")"
            echo "$CURRENT_CONTENT" > "$TMP_DIR/$CURRENT_FILE"
            FILES_LIST="$FILES_LIST $TMP_DIR/$CURRENT_FILE"
            FILE_COUNT=$((FILE_COUNT + 1))
        fi

        # Run tsz on all files from project dir
        if [ $FILE_COUNT -gt 0 ]; then
            cd "$ROOT_DIR"
            ACTUAL=$(timeout 10 "$TSZ_BIN" $FILES_LIST 2>&1 || true)
        else
            # No files extracted, try single file
            cd "$ROOT_DIR"
            ACTUAL=$(timeout 10 "$TSZ_BIN" "$TEST_FILE" 2>&1 || true)
        fi
    else
        # Single file test
        cd "$ROOT_DIR"
        ACTUAL=$(timeout 10 "$TSZ_BIN" "$TEST_FILE" 2>&1 || true)
    fi

    ACTUAL_CODES=$(echo "$ACTUAL" | grep -oE 'TS[0-9]+' | sort -u | tr '\n' ' ')

    # Cleanup temp dir
    rm -rf "$TMP_DIR"

    # Compare
    if [ "$EXPECTED_CODES" = "$ACTUAL_CODES" ]; then
        PASSED=$((PASSED + 1))
    else
        FAILED=$((FAILED + 1))
        echo "[FAIL] $BASENAME"
        echo "  Expected: $EXPECTED_CODES"
        echo "  Actual:   $ACTUAL_CODES"
    fi

    # Progress indicator
    if [ $((TOTAL % 10)) -eq 0 ]; then
        echo "Progress: $TOTAL/$MAX_TESTS (Pass: $PASSED, Fail: $FAILED)"
    fi
done

echo ""
echo "=============================="
echo "Results: $PASSED/$TOTAL passed ($FAILED failed)"
if [ $TOTAL -gt 0 ]; then
    RATE=$((PASSED * 100 / TOTAL))
    echo "Pass rate: $RATE%"
fi
