#!/bin/bash
#
# TSZ Conformance Test Runner
#
# Defaults to native Rust binary (fast). Use --wasm for WASM+Docker (isolated).
#
# Usage:
#   ./run-conformance.sh                    # Run 500 tests (native)
#   ./run-conformance.sh --wasm             # Run with WASM in Docker
#   ./run-conformance.sh --max=100          # Run 100 tests
#   ./run-conformance.sh --all              # Run all tests
#   ./run-conformance.sh --category=compiler # Run compiler tests only
#   ./run-conformance.sh --verbose          # Show detailed output

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Detect CPU cores
if [[ "$OSTYPE" == "darwin"* ]]; then
    CPU_CORES=$(sysctl -n hw.ncpu)
else
    CPU_CORES=$(nproc 2>/dev/null || echo 4)
fi

# Defaults
MAX_TESTS=500
USE_WASM=false
VERBOSE=false
CATEGORIES="conformance,compiler"

# Parse arguments
for arg in "$@"; do
    case $arg in
        --wasm) USE_WASM=true ;;
        --all) MAX_TESTS=99999 ;;
        --max=*) MAX_TESTS="${arg#*=}" ;;
        --verbose|-v) VERBOSE=true ;;
        --category=*) CATEGORIES="${arg#*=}" ;;
        --help|-h)
            echo "TSZ Conformance Test Runner"
            echo ""
            echo "Usage: ./run-conformance.sh [options]"
            echo ""
            echo "Options:"
            echo "  --wasm          Use WASM build in Docker (default: native binary)"
            echo "  --max=N         Run N tests (default: 500)"
            echo "  --all           Run all tests"
            echo "  --category=X    Test category: conformance, compiler, or both"
            echo "  --verbose, -v   Show detailed output"
            echo "  --help, -h      Show this help"
            echo ""
            echo "Native mode: Faster, uses cargo build --release"
            echo "WASM mode: Slower, uses Docker isolation for safety"
            exit 0
            ;;
    esac
done

# Branch based on mode
if [ "$USE_WASM" = true ]; then
    exec "$SCRIPT_DIR/run-wasm.sh" "$@"
else
    exec "$SCRIPT_DIR/run-native.sh" "$@"
fi

