#!/bin/bash
# Quick performance test for enumLiteralsSubtypeReduction.ts
# Tests the current .target/release/tsz binary

set -e

TSZ=${1:-.target/release/tsz}
TEST_FILE="TypeScript/tests/cases/compiler/enumLiteralsSubtypeReduction.ts"

if [ ! -f "$TSZ" ]; then
    echo "Error: tsz binary not found at $TSZ"
    echo "Build with: cargo build --release -p tsz-cli"
    exit 1
fi

if [ ! -f "$TEST_FILE" ]; then
    echo "Error: Test file not found: $TEST_FILE"
    exit 1
fi

echo "=== Quick Performance Test ==="
echo "Binary: $TSZ"
echo "File: $TEST_FILE"
echo ""

# Warmup run
echo "Warmup run..."
"$TSZ" "$TEST_FILE" > /dev/null 2>&1 || true

# Timed runs
echo ""
echo "Timing 3 runs..."
for i in 1 2 3; do
    start=$(perl -MTime::HiRes -e 'print Time::HiRes::time()')
    "$TSZ" "$TEST_FILE" > /dev/null 2>&1 || true
    end=$(perl -MTime::HiRes -e 'print Time::HiRes::time()')
    elapsed=$(echo "$end - $start" | bc)
    printf "Run %d: %.3f seconds\n" $i $elapsed
done

echo ""
echo "Done."
