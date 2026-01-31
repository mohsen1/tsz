#!/bin/bash
# Rust Test Runner
#
# Runs unit tests directly with resource protection (memory limits, timeouts).
#
# Usage:
#   ./scripts/test.sh                      # Run all tests
#   ./scripts/test.sh test_name            # Run specific test (pattern match)
#   ./scripts/test.sh --quick              # Quick mode: fail-fast, reduced threads
#   ./scripts/test.sh --ignored            # Include ignored tests
#   ./scripts/test.sh --timeout=60         # Kill after N seconds
#   ./scripts/test.sh --conformance        # Run conformance tests
#   ./scripts/test.sh --bench              # Run benchmarks
#
# Environment variables:
#   TSZ_TEST_VERBOSE=1    Show verbose output
#   TSZ_MAX_RSS_MB=8192   Max RSS in MB (default: 8192 = 8GB)

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Parse arguments
QUICK_MODE=false
CLEAN=false
TEST_FILTER=""
CONFORMANCE_TEST=""
CONFORMANCE_CATEGORY=""
RUN_IGNORED=false
TIMEOUT_SECS=""
RUN_BENCH=false
VERBOSE="${TSZ_TEST_VERBOSE:-}"

# Resource limits (protect host from runaway tests)
MAX_RSS_MB="${TSZ_MAX_RSS_MB:-8192}"  # 8GB default

while [[ $# -gt 0 ]]; do
    case $1 in
        --quick|-q) QUICK_MODE=true ;;
        --clean) CLEAN=true ;;
        --ignored) RUN_IGNORED=true ;;
        --timeout=*) TIMEOUT_SECS="${1#*=}" ;;
        --conformance) CONFORMANCE_TEST=true ;;
        --bench) RUN_BENCH=true ;;
        --verbose|-v) VERBOSE=1 ;;
        # Accept and ignore legacy Docker flags
        --sandbox|--docker|--no-sandbox|--no-docker|--rebuild) ;;
        compiler|conformance|projects)
            [[ "$CONFORMANCE_TEST" == true ]] && CONFORMANCE_CATEGORY="$1"
            ;;
        all)
            if [[ "$CONFORMANCE_TEST" == true ]]; then
                CONFORMANCE_CATEGORY="conformance,compiler,projects"
            else
                TEST_FILTER="$1"
            fi
            ;;
        *) TEST_FILTER="$1" ;;
    esac
    shift
done

# Delegate to conformance runner
if [[ "$CONFORMANCE_TEST" == true ]]; then
    if [[ -n "$CONFORMANCE_CATEGORY" ]]; then
        exec "$ROOT_DIR/conformance/run-conformance.sh" --category="$CONFORMANCE_CATEGORY"
    else
        exec "$ROOT_DIR/conformance/run-conformance.sh"
    fi
fi

# Delegate to bench runner
if [[ "$RUN_BENCH" == true ]]; then
    exec "$ROOT_DIR/scripts/bench.sh" "$TEST_FILTER"
fi

cd "$ROOT_DIR"

# Clean mode
if [[ "$CLEAN" == true ]]; then
    echo "Cleaning build artifacts..."
    cargo clean
    echo "Done"
    exit 0
fi

# Apply resource limits to protect the host system
# Set virtual memory limit (soft) to prevent runaway memory usage
MAX_RSS_KB=$((MAX_RSS_MB * 1024))
ulimit -v "$MAX_RSS_KB" 2>/dev/null || true

# Default timeout of 300s if not specified
if [[ -z "$TIMEOUT_SECS" ]]; then
    TIMEOUT_SECS=300
fi

# Check for nextest availability
HAS_NEXTEST=false
if command -v cargo-nextest &>/dev/null || cargo nextest --version &>/dev/null 2>&1; then
    HAS_NEXTEST=true
fi

# Build the command
if [[ "$HAS_NEXTEST" == true ]]; then
    CMD="cargo nextest run"
    [[ "$QUICK_MODE" == true ]] && CMD="$CMD --profile quick"
    [[ "$RUN_IGNORED" == true ]] && CMD="$CMD --run-ignored all"
    [[ -n "$TEST_FILTER" ]] && CMD="$CMD '$TEST_FILTER'"
else
    CMD="cargo test"
    [[ -n "$TEST_FILTER" ]] && CMD="$CMD '$TEST_FILTER'"
    [[ "$RUN_IGNORED" == true ]] && CMD="$CMD --include-ignored"
    [[ "$QUICK_MODE" == true ]] && CMD="$CMD -- --test-threads=4"
fi

# Wrap with timeout
CMD="timeout ${TIMEOUT_SECS}s $CMD"

[[ "$QUICK_MODE" == true ]] && [[ -z "$VERBOSE" ]] && echo "Quick mode (fail-fast)"
[[ -n "$VERBOSE" ]] && echo "Running: $CMD (memory limit: ${MAX_RSS_MB}MB, timeout: ${TIMEOUT_SECS}s)"

eval "$CMD"
EXIT_CODE=$?

if [[ $EXIT_CODE -eq 124 ]]; then
    echo "Tests timed out after ${TIMEOUT_SECS}s"
    exit 1
fi

exit $EXIT_CODE
