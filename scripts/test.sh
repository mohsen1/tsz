#!/bin/bash
# Rust Test Runner
#
# Runs tests in Docker by default for environment consistency (per AGENTS.md policy).
# Use --no-sandbox for fast local development when Docker overhead is unacceptable.
#
# Usage:
#   ./scripts/test.sh                      # Run all tests in Docker (default)
#   ./scripts/test.sh test_name            # Run specific test (pattern match)
#   ./scripts/test.sh --no-sandbox         # Fast mode: skip Docker for local dev
#   ./scripts/test.sh --quick              # Quick mode: fail-fast, reduced threads
#   ./scripts/test.sh --quick --no-sandbox # Fastest: no Docker + fail-fast
#   ./scripts/test.sh --ignored            # Include ignored tests
#   ./scripts/test.sh --timeout=60         # Kill after N seconds
#   ./scripts/test.sh --rebuild            # Force rebuild Docker image
#   ./scripts/test.sh --clean              # Clean cached volumes
#   ./scripts/test.sh --conformance        # Run conformance tests
#   ./scripts/test.sh --bench              # Run benchmarks
#
# Environment variables:
#   TSZ_TEST_DOCKER=1     Force Docker mode
#   TSZ_TEST_VERBOSE=1    Show verbose output

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Parse arguments
QUICK_MODE=false
USE_DOCKER=true
REBUILD=false
CLEAN=false
TEST_FILTER=""
CONFORMANCE_TEST=""
CONFORMANCE_CATEGORY=""
RUN_IGNORED=false
TIMEOUT_SECS=""
RUN_BENCH=false
VERBOSE="${TSZ_TEST_VERBOSE:-}"

# Fast argument parsing
while [[ $# -gt 0 ]]; do
    case $1 in
        --quick|-q) QUICK_MODE=true ;;
        --sandbox|--docker) USE_DOCKER=true ;;
        --no-sandbox) USE_DOCKER=false ;;  # Kept for backwards compat
        --rebuild) REBUILD=true; USE_DOCKER=true ;;
        --clean) CLEAN=true ;;
        --ignored) RUN_IGNORED=true ;;
        --timeout=*) TIMEOUT_SECS="${1#*=}" ;;
        --conformance) CONFORMANCE_TEST=true ;;
        --bench) RUN_BENCH=true ;;
        --verbose|-v) VERBOSE=1 ;;
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

# Environment override
[[ "${TSZ_TEST_DOCKER:-}" == "1" ]] && USE_DOCKER=true

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
    echo "Cleaning cached volumes..."
    docker volume rm cargo-registry cargo-git tsz-target-shared 2>/dev/null || true
    echo "Done"
    exit 0
fi

# =============================================================================
# FAST PATH: Direct execution (no Docker) - DEFAULT
# =============================================================================
if [[ "$USE_DOCKER" == false ]]; then
    # Check for nextest availability once
    HAS_NEXTEST=false
    if command -v cargo-nextest &>/dev/null || cargo nextest --version &>/dev/null 2>&1; then
        HAS_NEXTEST=true
    fi

    # Build the command directly as a string for speed
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

    # Add timeout wrapper if specified
    [[ -n "$TIMEOUT_SECS" ]] && CMD="timeout ${TIMEOUT_SECS}s $CMD"

    [[ "$QUICK_MODE" == true ]] && [[ -z "$VERBOSE" ]] && echo "Quick mode (fail-fast)"
    [[ -n "$VERBOSE" ]] && echo "Running: $CMD"

    eval "$CMD"
    exit $?
fi

# =============================================================================
# DOCKER PATH: Sandboxed execution
# =============================================================================
IMAGE_NAME="rust-wasm-base"
TARGET_VOLUME="tsz-target-shared"
DOCKER_MEMORY="8g"
DOCKER_CPUS="$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 8)"

echo "Running tests in Docker (mem: $DOCKER_MEMORY, cpus: $DOCKER_CPUS)"

# Build base image if needed
if [[ "$REBUILD" == true ]] || ! docker image inspect "$IMAGE_NAME" &>/dev/null; then
    echo "Building Docker image..."
    DOCKER_BUILDKIT=1 docker build -t "$IMAGE_NAME" -f - "$ROOT_DIR" << 'EOF'
# syntax=docker/dockerfile:1.4
FROM rust:latest
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo install cargo-nextest --locked
WORKDIR /app
EOF
fi

# Build nextest args
NEXTEST_CMD="cargo nextest run"
[[ "$RUN_IGNORED" == true ]] && NEXTEST_CMD="$NEXTEST_CMD --run-ignored all"
[[ -n "$TIMEOUT_SECS" ]] && NEXTEST_CMD="timeout ${TIMEOUT_SECS}s $NEXTEST_CMD"
[[ -n "$TEST_FILTER" ]] && NEXTEST_CMD="$NEXTEST_CMD $TEST_FILTER"

# Use tar for fast incremental file sync (much faster than find+cp)
# Only syncs files that changed, excludes target dir and git
SYNC_CMD='tar -C /source --exclude=.target --exclude=target --exclude=.git -cf - . | tar -C /app -xf -'

docker run --rm \
    --memory="$DOCKER_MEMORY" \
    --cpus="$DOCKER_CPUS" \
    -v "$ROOT_DIR:/source:ro" \
    -v cargo-registry:/usr/local/cargo/registry \
    -v cargo-git:/usr/local/cargo/git \
    -v "$TARGET_VOLUME":/app/target \
    -e CARGO_INCREMENTAL=1 \
    "$IMAGE_NAME" \
    bash -c "$SYNC_CMD && $NEXTEST_CMD"

echo "Done"
