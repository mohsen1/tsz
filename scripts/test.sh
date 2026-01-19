#!/bin/bash
# Rust Test Runner - Docker-based testing with caching
#
# This is the main test runner for the Rust/WASM TypeScript implementation.
# It uses Docker to ensure consistent testing environment and caches builds
# for fast iteration.
#
# Usage:
#   ./scripts/test.sh                          # Run all Rust unit tests
#   ./scripts/test.sh test_name                # Run specific test
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

set -e

IMAGE_NAME="rust-wasm-base"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Parse arguments
REBUILD=false
CLEAN=false
TEST_FILTER=""
CONFORMANCE_TEST=""
CONFORMANCE_CATEGORY=""

for arg in "$@"; do
    case $arg in
        --rebuild)
            REBUILD=true
            ;;
        --clean)
            CLEAN=true
            ;;
        --conformance)
            CONFORMANCE_TEST=true
            ;;
        --all)
            CONFORMANCE_CATEGORY="conformance,compiler,projects"
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

# If conformance test requested, delegate to conformance runner
if [ "$CONFORMANCE_TEST" = true ]; then
    if [ -n "$CONFORMANCE_CATEGORY" ]; then
        exec "$ROOT_DIR/conformance/run-conformance.sh" --category="$CONFORMANCE_CATEGORY"
    else
        exec "$ROOT_DIR/conformance/run-conformance.sh"
    fi
fi

# Clean cached volumes if requested
if [ "$CLEAN" = true ]; then
    echo "🧹 Cleaning cached volumes..."
    docker volume rm cargo-registry cargo-git wasm-target-cache 2>/dev/null || true
    echo "✅ Volumes cleaned"
    exit 0
fi

# Build base image if needed or forced (only contains rustc + nextest, not source)
if [ "$REBUILD" = true ] || ! docker image inspect "$IMAGE_NAME" &>/dev/null; then
    echo "🔨 Building base Docker image..."
    docker build -t "$IMAGE_NAME" -f - "$ROOT_DIR" << 'EOF'
FROM rust:latest
RUN cargo install cargo-nextest --locked
WORKDIR /app
EOF
fi

# Run tests
echo "🧪 Running tests..."

# We use a workaround: copy source to a writable location inside the container
# This avoids the ro mount conflict with the target cache
if [ -n "$TEST_FILTER" ]; then
    echo "   Filter: $TEST_FILTER"
    docker run --rm --memory="2g" --cpus="2.0" \
        -v "$ROOT_DIR:/source:ro" \
        -v cargo-registry:/usr/local/cargo/registry \
        -v cargo-git:/usr/local/cargo/git \
        "$IMAGE_NAME" bash -c "rm -rf /app/* && cp -r /source/* /app/ && cargo nextest run $TEST_FILTER"
else
    docker run --rm --memory="2g" --cpus="2.0" \
        -v "$ROOT_DIR:/source:ro" \
        -v cargo-registry:/usr/local/cargo/registry \
        -v cargo-git:/usr/local/cargo/git \
        "$IMAGE_NAME" bash -c "rm -rf /app/* && cp -r /source/* /app/ && cargo nextest run"
fi

echo "✅ Tests complete!"
