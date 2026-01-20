#!/bin/bash
#
# TSZ Conformance Test Runner
# 
# Runs TypeScript conformance tests in Docker for safety.
# Tests can cause infinite loops or OOM - Docker provides isolation.
#
# Usage:
#   ./run-conformance.sh                    # Run 500 tests
#   ./run-conformance.sh --max=100          # Run 100 tests  
#   ./run-conformance.sh --all              # Run all tests
#   ./run-conformance.sh --category=compiler # Run compiler tests only
#   ./run-conformance.sh --verbose          # Show detailed output
#   ./run-conformance.sh --rebuild          # Rebuild Docker image

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
IMAGE_NAME="tsz-conformance"

# Detect CPU cores
if [[ "$OSTYPE" == "darwin"* ]]; then
    CPU_CORES=$(sysctl -n hw.ncpu)
else
    CPU_CORES=$(nproc 2>/dev/null || echo 4)
fi

# Defaults - use all cores
MAX_TESTS=500
REBUILD=false
VERBOSE=false
CATEGORIES="conformance,compiler"
TIMEOUT=600  # 10 minutes default
WORKERS=$CPU_CORES  # Use all CPU cores

# Parse arguments
for arg in "$@"; do
    case $arg in
        --rebuild) REBUILD=true ;;
        --all) MAX_TESTS=99999; TIMEOUT=3600 ;;
        --max=*) MAX_TESTS="${arg#*=}" ;;
        --workers=*) WORKERS="${arg#*=}" ;;
        --verbose|-v) VERBOSE=true ;;
        --category=*) CATEGORIES="${arg#*=}" ;;
        --timeout=*) TIMEOUT="${arg#*=}" ;;
        --help|-h)
            echo "TSZ Conformance Test Runner"
            echo ""
            echo "Usage: ./run-conformance.sh [options]"
            echo ""
            echo "Options:"
            echo "  --max=N         Run N tests (default: 500)"
            echo "  --workers=N     Number of parallel workers (default: 8)"
            echo "  --all           Run all tests"
            echo "  --category=X    Test category: conformance, compiler, or both"
            echo "  --verbose, -v   Show detailed output"
            echo "  --timeout=S     Timeout in seconds (default: 600)"
            echo "  --rebuild       Force rebuild Docker image"
            echo "  --help, -h      Show this help"
            exit 0
            ;;
    esac
done

# Check Docker is available
if ! command -v docker &> /dev/null; then
    echo "❌ Docker is required but not installed."
    echo "   Install Docker: https://docs.docker.com/get-docker/"
    exit 1
fi

# Check Docker daemon is running
if ! docker info &> /dev/null; then
    echo "❌ Docker daemon is not running."
    echo "   Start Docker Desktop or run: sudo systemctl start docker"
    exit 1
fi

echo "╔══════════════════════════════════════════════════════════╗"
echo "║      TSZ Parallel Conformance Test Runner (Docker)       ║"
echo "╠══════════════════════════════════════════════════════════╣"
echo "║  Tests:      $(printf '%-43s' "$MAX_TESTS") ║"
echo "║  Workers:    $(printf '%-43s' "$WORKERS") ║"
echo "║  Categories: $(printf '%-43s' "$CATEGORIES") ║"
echo "║  Timeout:    $(printf '%-43s' "${TIMEOUT}s") ║"
echo "╚══════════════════════════════════════════════════════════╝"

# Build TypeScript runner
echo ""
echo "📦 Building conformance runner..."
cd "$SCRIPT_DIR"
if [ ! -d "node_modules" ]; then
    npm install --silent
fi
npm run build --silent
cd "$ROOT_DIR"
echo "✅ Runner built"

# Build Docker image if needed
if [ "$REBUILD" = true ] || ! docker image inspect "$IMAGE_NAME" &>/dev/null; then
    echo ""
    echo "🐳 Building Docker image..."
    docker build -t "$IMAGE_NAME" -f - "$SCRIPT_DIR" << 'DOCKERFILE'
FROM node:22-slim
WORKDIR /app
RUN mkdir -p /app/conformance /app/pkg /app/TypeScript/tests
DOCKERFILE
    echo "✅ Docker image built"
fi

# Calculate memory: ~1.5GB per worker, minimum 4GB
# Use MEMORY_GB from environment if set, otherwise calculate
if [ -z "$MEMORY_GB" ]; then
    MEMORY_GB=$(( WORKERS * 3 / 2 ))
fi
if [ $MEMORY_GB -lt 4 ]; then MEMORY_GB=4; fi

echo ""
echo "🚀 Running tests in Docker container..."
echo "   (Memory: ${MEMORY_GB}GB, CPUs: $WORKERS, Timeout: ${TIMEOUT}s)"
echo ""

# Build runner args
RUNNER_ARGS="--max=$MAX_TESTS --workers=$WORKERS --category=$CATEGORIES"
if [ "$VERBOSE" = true ]; then
    RUNNER_ARGS="$RUNNER_ARGS --verbose"
fi

# Run tests in Docker with resource limits
# Memory: ~1.5GB per worker, CPUs: match worker count
docker run --rm \
    --memory="${MEMORY_GB}g" \
    --memory-swap="${MEMORY_GB}g" \
    --cpus="$WORKERS" \
    --pids-limit=1000 \
    -v "$ROOT_DIR/pkg:/app/pkg:ro" \
    -v "$SCRIPT_DIR/src:/app/conformance/src:ro" \
    -v "$SCRIPT_DIR/dist:/app/conformance/dist:ro" \
    -v "$SCRIPT_DIR/package.json:/app/conformance/package.json:ro" \
    -v "$ROOT_DIR/TypeScript/tests:/app/TypeScript/tests:ro" \
    "$IMAGE_NAME" sh -c "
        cd /app/conformance
        npm install --silent 2>/dev/null || true
        timeout ${TIMEOUT}s node --expose-gc dist/runner.js $RUNNER_ARGS
        EXIT_CODE=\$?
        if [ \$EXIT_CODE -eq 124 ]; then
            echo ''
            echo '⏱️  Tests timed out after ${TIMEOUT}s'
        fi
        exit \$EXIT_CODE
    "

echo ""
echo "✅ Done!"
