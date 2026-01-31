#!/usr/bin/env bash
# Benchmark Script
#
# Runs Rust benchmarks with resource protection (memory limits, timeouts).
#
# Usage:
#   ./scripts/bench.sh                  # Run all benchmarks
#   ./scripts/bench.sh emitter_bench    # Run specific benchmark
#
# Environment variables:
#   TSZ_MAX_RSS_MB=8192   Max RSS in MB (default: 8192 = 8GB)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

MAX_RSS_MB="${TSZ_MAX_RSS_MB:-8192}"

echo "Running benchmarks (memory limit: ${MAX_RSS_MB}MB)..."

# Apply resource limits to protect the host system
MAX_RSS_KB=$((MAX_RSS_MB * 1024))
ulimit -v "$MAX_RSS_KB" 2>/dev/null || true

# Determine which benchmarks to run
BENCH_FILTER="${1:-}"
BENCH_ARGS=""
if [ -n "$BENCH_FILTER" ]; then
    BENCH_ARGS="--bench $BENCH_FILTER"
    echo "Running benchmark: $BENCH_FILTER"
else
    echo "Running all benchmarks"
fi

cd "$PROJECT_ROOT"

# Run benchmarks with a timeout to prevent runaway execution
timeout 600s cargo bench $BENCH_ARGS --color=always
EXIT_CODE=$?

if [ $EXIT_CODE -eq 124 ]; then
    echo "Benchmarks timed out after 600s"
    exit 1
fi

echo "Benchmarks complete!"
echo ""
echo "Benchmark results are saved in: target/criterion/"
echo "To view HTML reports, open: target/criterion/report/index.html"

exit $EXIT_CODE
