#!/bin/bash
#
# Native Conformance Test Runner (Unsafe - No Docker)
#
# Internal script called by run-conformance.sh for --no-sandbox mode.
# Runs native binary directly without Docker isolation.
# WARNING: Faster but vulnerable to infinite loops/OOM bugs.
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Parse args (passed through from parent)
MAX_TESTS=500
USE_WASM=false
VERBOSE=false
CATEGORIES="conformance,compiler"
TIMEOUT=10
WORKERS=

for arg in "$@"; do
    case $arg in
        --wasm=*) USE_WASM="${arg#*=}" ;;
        --wasm) USE_WASM=true ;;
        --max=*) MAX_TESTS="${arg#*=}" ;;
        --verbose|-v) VERBOSE=true ;;
        --category=*) CATEGORIES="${arg#*=}" ;;
        --timeout=*) TIMEOUT="${arg#*=}" ;;
        --workers=*) WORKERS="${arg#*=}" ;;
    esac
done

# Default to 8 workers (optimal for WASM - more causes contention)
if [ -z "$WORKERS" ]; then
    WORKERS=8
fi

MODE_DESC="$([ "$USE_WASM" = true ] && echo "WASM" || echo "Native Binary")"
echo "╔══════════════════════════════════════════════════════════╗"
echo "║   TSZ Conformance Runner (Unsafe - No Docker)            ║"
echo "╠══════════════════════════════════════════════════════════╣"
echo "║  Mode:       $(printf '%-43s' "$MODE_DESC") ║"
echo "║  Tests:      $(printf '%-43s' "$MAX_TESTS") ║"
echo "║  Workers:    $(printf '%-43s' "$WORKERS") ║"
echo "║  Categories: $(printf '%-43s' "$CATEGORIES") ║"
echo "╚══════════════════════════════════════════════════════════╝"
echo ""
echo "⚠️  WARNING: Running without Docker isolation."
echo "   Vulnerable to infinite loops and OOM bugs."
echo ""

# Build target
if [ "$USE_WASM" = true ]; then
    echo "📦 Building WASM..."
    cd "$ROOT_DIR"
    wasm-pack build --target nodejs --out-dir pkg
    echo "✅ WASM built"
else
    echo "📦 Building native binary..."
    cd "$ROOT_DIR"
    cargo build --release --bin tsz
    echo "✅ Native binary built"
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

echo ""
echo "🚀 Running tests ($MODE_DESC)..."
echo "   (Workers: $WORKERS, Timeout: ${TIMEOUT}s)"
echo ""

# Build runner args
RUNNER_ARGS="--max=$MAX_TESTS --workers=$WORKERS --category=$CATEGORIES --wasm=$USE_WASM"
if [ "$VERBOSE" = true ]; then
    RUNNER_ARGS="$RUNNER_ARGS --verbose"
fi

# Run the runner
node --expose-gc dist/runner.js $RUNNER_ARGS

echo ""
echo "✅ Done!"
