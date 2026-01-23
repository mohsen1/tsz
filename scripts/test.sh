#!/bin/bash
# Rust Test Runner - Docker-based testing with caching
#
# This is the main test runner for the Rust/WASM TypeScript implementation.
# It uses Docker to ensure consistent testing environment and caches builds
# for fast iteration.
#
# Usage:
#   ./scripts/test.sh                          # Run all Rust unit tests (in Docker)
#   ./scripts/test.sh --no-sandbox             # Run tests directly without Docker
#   ./scripts/test.sh test_name                # Run specific test (pattern match)
#   ./scripts/test.sh namespace               # Run all tests matching "namespace"
#   ./scripts/test.sh --ignored test_name      # Run ignored tests too (nextest: --run-ignored all)
#   ./scripts/test.sh --timeout=60 test_name   # Kill the test run after N seconds (inside container)
#   ./scripts/test.sh --rebuild                # Force rebuild Docker image
#   ./scripts/test.sh --clean                  # Clean cached volumes
#   ./scripts/test.sh --bench                  # Run benchmarks
#   ./scripts/test.sh --conformance            # Run conformance tests
#   ./scripts/test.sh --conformance compiler   # Run compiler category tests
#   ./scripts/test.sh --conformance all        # Run all conformance categories
#
# For TypeScript test suite conformance testing, you can also use:
#   ./conformance/run-conformance.sh
#
# Source code is always mounted fresh (not baked into image), so file changes
# are immediately visible without needing to rebuild the Docker image.

set -euo pipefail

IMAGE_NAME="rust-wasm-base"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Use shared target volume for all worktrees to prevent duplicate build artifacts
# When running with orchestrator, all workers share the same build cache
# For standalone runs, we still use the shared volume for efficiency
TARGET_VOLUME="tsz-target-shared"

# Resource limits - use most available cores for parallel test execution
DOCKER_MEMORY="8g"
DOCKER_CPUS="$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 8)"

# Parse arguments
REBUILD=false
CLEAN=false
TEST_FILTER=""
CONFORMANCE_TEST=""
CONFORMANCE_CATEGORY=""
RUN_IGNORED=false
TIMEOUT_SECS=""
USE_DOCKER=true

for arg in "$@"; do
    case $arg in
        --no-sandbox)
            USE_DOCKER=false
            ;;
        --rebuild)
            REBUILD=true
            ;;
        --clean)
            CLEAN=true
            ;;
        --ignored)
            RUN_IGNORED=true
            ;;
        --timeout=*)
            TIMEOUT_SECS="${arg#*=}"
            ;;
        --conformance)
            CONFORMANCE_TEST=true
            ;;
        compiler|conformance|projects)
            if [ "$CONFORMANCE_TEST" = true ]; then
                CONFORMANCE_CATEGORY="$arg"
            fi
            ;;
        *)
            TEST_FILTER="$arg"
            ;;
    esac
done

# Handle --all flag for conformance tests
if [ "$CONFORMANCE_TEST" = true ] && [ "$TEST_FILTER" = "all" ]; then
    CONFORMANCE_CATEGORY="conformance,compiler,projects"
    TEST_FILTER=""
fi

# If conformance test requested, delegate to conformance runner
if [ "$CONFORMANCE_TEST" = true ]; then
    if [ -n "$CONFORMANCE_CATEGORY" ]; then
        exec "$ROOT_DIR/conformance/run-conformance.sh" --category="$CONFORMANCE_CATEGORY"
    else
        exec "$ROOT_DIR/conformance/run-conformance.sh"
    fi
fi

# Run tests directly if --no-sandbox is specified
if [ "$USE_DOCKER" = false ]; then
    echo "🧪 Running tests WITHOUT Docker sandbox..."
    echo "⚠️  Warning: Tests may consume significant memory and could crash your host"

    cd "$ROOT_DIR"

    # Build nextest args
    NEXT_TEST_ARGS=""
    if [ "$RUN_IGNORED" = true ]; then
        NEXT_TEST_ARGS="$NEXT_TEST_ARGS --run-ignored all"
    fi

    # Build the cargo command
    if [ -n "$TIMEOUT_SECS" ]; then
        echo "   Timeout: ${TIMEOUT_SECS}s"
        CARGO_CMD="timeout ${TIMEOUT_SECS}s cargo nextest run$NEXT_TEST_ARGS"
    else
        CARGO_CMD="cargo nextest run$NEXT_TEST_ARGS"
    fi

    if [ -n "$TEST_FILTER" ]; then
        echo "   Filter: $TEST_FILTER"
        eval "$CARGO_CMD $TEST_FILTER"
    else
        eval "$CARGO_CMD"
    fi

    echo "✅ Tests complete!"
    exit 0
fi

# Clean cached volumes if requested
if [ "$CLEAN" = true ]; then
    echo "🧹 Cleaning cached volumes for this worktree..."
    echo "   Target volume: $TARGET_VOLUME"
    docker volume rm cargo-registry cargo-git "$TARGET_VOLUME" 2>/dev/null || true
    echo "✅ Volumes cleaned"
    exit 0
fi

# Build base image if needed or forced (only contains rustc + nextest, not source)
if [ "$REBUILD" = true ] || ! docker image inspect "$IMAGE_NAME" &>/dev/null; then
    echo "🔨 Building base Docker image..."
    DOCKER_BUILDKIT=1 docker build -t "$IMAGE_NAME" -f - "$ROOT_DIR" << 'EOF'
# syntax=docker/dockerfile:1.4
FROM rust:latest
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo install cargo-nextest --locked
WORKDIR /app
EOF
fi

# Run tests
echo "🧪 Running tests in Docker sandbox (memory: $DOCKER_MEMORY, cpus: $DOCKER_CPUS)..."
echo "   Use --no-sandbox to run tests directly without Docker"
echo "   Target cache: $TARGET_VOLUME (shared across all worktrees)"

# We use a workaround: copy source to a writable location inside the container
# This avoids the ro mount conflict with the target cache
NEXT_TEST_ARGS=""
if [ "$RUN_IGNORED" = true ]; then
    NEXT_TEST_ARGS="$NEXT_TEST_ARGS --run-ignored all"
fi
if [ -n "$TIMEOUT_SECS" ]; then
    echo "   Timeout: ${TIMEOUT_SECS}s"
    NEXT_TEST_ARGS="timeout ${TIMEOUT_SECS}s cargo nextest run$NEXT_TEST_ARGS"
else
    NEXT_TEST_ARGS="cargo nextest run$NEXT_TEST_ARGS"
fi

# Copy source files excluding target directory (which is a mounted volume)
COPY_CMD="find /app -mindepth 1 -maxdepth 1 ! -name target -exec rm -rf {} + && find /source -mindepth 1 -maxdepth 1 ! -name target -exec cp -r {} /app/ \\;"

if [ -n "$TEST_FILTER" ]; then
    echo "   Filter: $TEST_FILTER"
    docker run --rm --memory="$DOCKER_MEMORY" --cpus="$DOCKER_CPUS" \
        -v "$ROOT_DIR:/source:ro" \
        -v cargo-registry:/usr/local/cargo/registry \
        -v cargo-git:/usr/local/cargo/git \
        -v "$TARGET_VOLUME":/app/target \
        "$IMAGE_NAME" bash -c "$COPY_CMD && $NEXT_TEST_ARGS $TEST_FILTER"
else
    docker run --rm --memory="$DOCKER_MEMORY" --cpus="$DOCKER_CPUS" \
        -v "$ROOT_DIR:/source:ro" \
        -v cargo-registry:/usr/local/cargo/registry \
        -v cargo-git:/usr/local/cargo/git \
        -v "$TARGET_VOLUME":/app/target \
        "$IMAGE_NAME" bash -c "$COPY_CMD && $NEXT_TEST_ARGS"
fi

echo "✅ Tests complete!"
