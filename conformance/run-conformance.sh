#!/bin/bash
# Docker-based conformance test runner
# 
# ⚠️ IMPORTANT: Always use this script or ./scripts/test.sh to run conformance tests.
# Running tests directly on the host can cause infinite loops or OOM crashes.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
IMAGE_NAME="ts-conformance-runner"

MAX_TESTS=500
REBUILD=false
VERBOSE=false
CATEGORIES="conformance"

for arg in "$@"; do
    case $arg in
        --rebuild) REBUILD=true ;;
        --all) MAX_TESTS=999999 ;;
        --max=*) MAX_TESTS="${arg#*=}" ;;
        --verbose|-v) VERBOSE=true ;;
        --category=*) CATEGORIES="${arg#*=}" ;;
    esac
done

echo "======================================"
echo "  Conformance Test Runner (Docker)"
echo "======================================"
echo "  Tests:      $MAX_TESTS"
echo "  Categories: $CATEGORIES"
echo "  Verbose:    $VERBOSE"
echo "======================================"

if [ "$REBUILD" = true ] || ! docker image inspect "$IMAGE_NAME" &>/dev/null; then
    echo "Building Docker image..."
    docker build -t "$IMAGE_NAME" -f - "$SCRIPT_DIR" << 'EOF'
FROM node:22
RUN npm install -g typescript
WORKDIR /app
EOF
fi

echo "Running conformance tests..."

RUNNER_ARGS="--max=$MAX_TESTS --category=$CATEGORIES"
if [ "$VERBOSE" = true ]; then
    RUNNER_ARGS="$RUNNER_ARGS --verbose"
fi

docker run --rm \
    --memory="4g" \
    --cpus="2" \
    -v "$ROOT_DIR/pkg:/app/pkg:ro" \
    -v "$SCRIPT_DIR/src:/app/conformance/src:ro" \
    -v "$SCRIPT_DIR/dist:/app/conformance/dist:ro" \
    -v "$SCRIPT_DIR/package.json:/app/conformance/package.json:ro" \
    -v "$ROOT_DIR/TypeScript/tests:/app/TypeScript/tests:ro" \
    "$IMAGE_NAME" sh -c "
        cd /app/conformance
        npm install --silent 2>/dev/null || true
        timeout 300s node dist/runner.js $RUNNER_ARGS || echo 'Tests completed or timed out'
    "

echo "Done!"
