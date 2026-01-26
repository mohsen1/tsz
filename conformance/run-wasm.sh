#!/bin/bash
#
# WASM Conformance Test Runner (Docker-isolated)
#
# Internal script called by run-conformance.sh when --wasm is used.
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
IMAGE_NAME="tsz-conformance"

# Parse args (passed through from parent)
MAX_TESTS=500
VERBOSE=false
CATEGORIES="conformance,compiler"
TIMEOUT=600
WORKERS=

for arg in "$@"; do
    case $arg in
        --max=*) MAX_TESTS="${arg#*=}" ;;
        --verbose|-v) VERBOSE=true ;;
        --category=*) CATEGORIES="${arg#*=}" ;;
        --timeout=*) TIMEOUT="${arg#*=}" ;;
        --workers=*) WORKERS="${arg#*=}" ;;
    esac
done

# Detect CPU cores if not specified
if [ -z "$WORKERS" ]; then
    if [[ "$OSTYPE" == "darwin"* ]]; then
        WORKERS=$(sysctl -n hw.ncpu)
    else
        WORKERS=$(nproc 2>/dev/null || echo 4)
    fi
fi

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
echo "║      TSZ Conformance Runner (WASM + Docker)             ║"
echo "╠══════════════════════════════════════════════════════════╣"
echo "║  Tests:      $(printf '%-43s' "$MAX_TESTS") ║"
echo "║  Workers:    $(printf '%-43s' "$WORKERS") ║"
echo "║  Categories: $(printf '%-43s' "$CATEGORIES") ║"
echo "║  Timeout:    $(printf '%-43s' "${TIMEOUT}s") ║"
echo "╚══════════════════════════════════════════════════════════╝"

# Build WASM
echo ""
echo "📦 Building WASM..."
cd "$ROOT_DIR"
wasm-pack build --target nodejs --out-dir pkg --release
echo "✅ WASM built"

LIB_SRC="$ROOT_DIR/TypeScript/lib"
if [ -d "$LIB_SRC" ]; then
    echo "📦 Copying TypeScript lib files (packaged)..."
    rm -rf "$ROOT_DIR/pkg/lib"
    mkdir -p "$ROOT_DIR/pkg/lib"
    cp -R "$LIB_SRC/." "$ROOT_DIR/pkg/lib/"
elif [ -d "$ROOT_DIR/TypeScript/src/lib" ]; then
    echo "📦 Copying TypeScript lib files (source)..."
    rm -rf "$ROOT_DIR/pkg/lib"
    mkdir -p "$ROOT_DIR/pkg/lib"
    cp -R "$ROOT_DIR/TypeScript/src/lib/." "$ROOT_DIR/pkg/lib/"
else
    echo "⚠️  TypeScript lib directory not found; skipping lib copy"
fi

# Build TypeScript runner
echo ""
echo "📦 Building conformance runner..."
cd "$SCRIPT_DIR"
if [ ! -d "node_modules" ]; then
    npm install --silent
fi
npm run build --silent
echo "✅ Runner built"

# Build Docker image if needed
echo ""
echo "🐳 Preparing Docker image..."
if ! docker image inspect "$IMAGE_NAME" &>/dev/null; then
    docker build -t "$IMAGE_NAME" -f - "$SCRIPT_DIR" << 'DOCKERFILE'
FROM node:22-slim
WORKDIR /app
RUN mkdir -p /app/conformance /app/pkg /app/TypeScript/tests
DOCKERFILE
fi
echo "✅ Docker ready"

# Calculate memory: ~1.5GB per worker, minimum 4GB
MEMORY_GB=$(( WORKERS * 3 / 2 ))
if [ $MEMORY_GB -lt 4 ]; then MEMORY_GB=4; fi

echo ""
echo "🚀 Running tests in Docker..."
echo "   (Memory: ${MEMORY_GB}GB, CPUs: $WORKERS, Timeout: ${TIMEOUT}s)"
echo ""

# Build runner args
RUNNER_ARGS="--max=$MAX_TESTS --workers=$WORKERS --category=$CATEGORIES"
if [ "$VERBOSE" = true ]; then
    RUNNER_ARGS="$RUNNER_ARGS --verbose"
fi

# Run tests in Docker with resource limits
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
