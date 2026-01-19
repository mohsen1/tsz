#!/bin/bash
# Docker-based conformance test runner

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WASM_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT_DIR="$WASM_DIR"
IMAGE_NAME="ts-conformance-runner"

MAX_TESTS=500
WORKERS=14
REBUILD=false
PARALLEL=true
CATEGORIES="conformance"

for arg in "$@"; do
    case $arg in
        --rebuild) REBUILD=true ;;
        --all) MAX_TESTS=999999 ;;
        --max=*) MAX_TESTS="${arg#*=}" ;;
        --workers=*) WORKERS="${arg#*=}" ;;
        --sequential) PARALLEL=false ;;
        --category=*) CATEGORIES="${arg#*=}" ;;
    esac
done

echo "======================================"
echo "  Conformance Test Runner (Docker)"
echo "======================================"
echo "  Tests:     $MAX_TESTS"
echo "  Workers:   $WORKERS"
echo "  Categories: $CATEGORIES"
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

if [ "$PARALLEL" = true ]; then
    RUNNER_SCRIPT="process-pool-conformance.mjs"
    RUNNER_ARGS="--max=$MAX_TESTS --workers=$WORKERS --category=$CATEGORIES"
else
    RUNNER_SCRIPT="conformance-runner.mjs"
    RUNNER_ARGS="--max=$MAX_TESTS --category=$CATEGORIES"
fi

docker run --rm \
    --memory="8g" \
    --cpus="$WORKERS" \
    -v "$WASM_DIR/pkg:/wasm-pkg:ro" \
    -v "$SCRIPT_DIR:/runner-src:ro" \
    -v "$ROOT_DIR/TypeScript/tests:/ts-tests:ro" \
    "$IMAGE_NAME" sh -c "
        # Create structure that matches runner paths:
        # __dirname = /app/conformance
        # wasmPkgPath = resolve(__dirname, '../pkg') = /app/pkg
        # conformanceDir = resolve(__dirname, '../TypeScript/tests/cases/conformance') = /app/ts-tests/cases/conformance
        # libPath = resolve(__dirname, '../TypeScript/tests/lib/lib.d.ts') = /app/ts-tests/lib/lib.d.ts
        mkdir -p /app/conformance /app/pkg /app/ts-tests/cases /app/ts-tests/lib
        cp -r /wasm-pkg/* /app/pkg/
        cp -r /runner-src/*.mjs /runner-src/*.js /runner-src/package.json /app/conformance/ 2>/dev/null || true
        cp -rL /ts-tests/cases/conformance /app/ts-tests/cases/ 2>/dev/null || true
        cp -rL /ts-tests/lib/* /app/ts-tests/lib/ 2>/dev/null || true
        cd /app/conformance
        npm install --silent 2>/dev/null || true
        node $RUNNER_SCRIPT $RUNNER_ARGS
    "

echo "Done!"
