#!/bin/bash
#
# Native Conformance Test Runner
#
# Internal script called by run-conformance.sh for native binary (default).
# Faster than WASM but no Docker isolation.
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Parse args (passed through from parent)
MAX_TESTS=500
VERBOSE=false
CATEGORIES="conformance,compiler"

for arg in "$@"; do
    case $arg in
        --max=*) MAX_TESTS="${arg#*=}" ;;
        --verbose|-v) VERBOSE=true ;;
        --category=*) CATEGORIES="${arg#*=}" ;;
    esac
done

echo "╔══════════════════════════════════════════════════════════╗"
echo "║      TSZ Conformance Runner (Native Binary)             ║"
echo "╠══════════════════════════════════════════════════════════╣"
echo "║  Tests:      $(printf '%-43s' "$MAX_TESTS") ║"
echo "║  Categories: $(printf '%-43s' "$CATEGORIES") ║"
echo "╚══════════════════════════════════════════════════════════╝"

# Build native binary
echo ""
echo "📦 Building native binary..."
cd "$ROOT_DIR"
cargo build --release --bin tsz
echo "✅ Native binary built"

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
echo "🚀 Running tests..."
echo ""

# Set environment variables for the runner
export TSZ_BINARY="$ROOT_DIR/target/release/tsz"
export MAX_TESTS
export CATEGORIES
export VERBOSE

# Run the native runner
node --expose-gc dist/runner-native.js

echo ""
echo "✅ Done!"
