#!/bin/bash
#
# TSZ Conformance Test Runner
#
# Defaults to Docker-isolated WASM for safety. Use --no-sandbox for native binary.
#
# Usage:
#   ./run-conformance.sh                    # Run 500 tests (Docker+WASM, safe)
#   ./run-conformance.sh --no-sandbox      # Run with native binary (faster, risky)
#   ./run-conformance.sh --wasm            # Use WASM (default) in Docker
#   ./run-conformance.sh --all              # Run all tests
#   ./run-conformance.sh --max=100          # Run 100 tests
#   ./run-conformance.sh --category=compiler # Run compiler tests only
#   ./run-conformance.sh --verbose          # Show detailed output

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Handle cache commands first (before anything else)
if [[ "${1:-}" == cache:* ]]; then
    CACHE_CMD="${1#cache:}"
    cd "$SCRIPT_DIR"

    # Build conformance runner if needed
    if [ ! -d "node_modules" ] || [ ! -d "node_modules/typescript" ]; then
        npm install --silent 2>/dev/null
    fi
    npm run build --silent 2>/dev/null

    case "$CACHE_CMD" in
        generate)
            node dist/generate-cache.js
            ;;
        status)
            node dist/generate-cache.js --status
            ;;
        clear)
            node dist/generate-cache.js --clear
            ;;
        *)
            echo "Unknown cache command: $CACHE_CMD"
            echo "Available: cache:generate, cache:status, cache:clear"
            exit 1
            ;;
    esac
    exit 0
fi

# Defaults
MAX_TESTS=500
USE_SANDBOX=true  # Docker by default
USE_WASM=true     # WASM by default
VERBOSE=false
CATEGORIES="conformance,compiler,projects"
TIMEOUT=600
WORKERS=8  # Optimal for WASM - more workers cause contention

# Parse arguments
SawAll=false
for arg in "$@"; do
    case $arg in
        --no-sandbox) USE_SANDBOX=false ;;
        --wasm) USE_WASM=true ;;
        --native) USE_WASM=false ;;
        --all) SawAll=true; MAX_TESTS=99999; TIMEOUT=3600 ;;
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
            echo "  --no-sandbox    Use native binary without Docker (faster, risky)"
            echo "  --wasm          Use WASM in Docker (default: true)"
            echo "  --native        Use native binary in Docker (faster, still isolated)"
            echo "  --max=N         Run N tests (default: 500)"
            echo "  --all           Run all tests"
            echo "  --category=X    Test category: conformance, compiler, projects, or comma-separated list"
            echo "  --verbose, -v   Show detailed output"
            echo "  --help, -h      Show this help"
            echo ""
            echo "Modes:"
            echo "  Default (no flags):        Docker + WASM (safe, slower)"
            echo "  --native:                 Docker + Native binary (safe, faster)"
            echo "  --no-sandbox:              Native binary directly (fastest, risky)"
            echo ""
            echo "Safety: Docker provides isolation from infinite loops/OOM."
            echo "Use --native in Docker for speed while maintaining safety."
            exit 0
            ;;
    esac
done

# Build arguments to pass to child scripts, converting --all to --max and --timeout
CHILD_ARGS=()
for arg in "$@"; do
    case $arg in
        --all)
            # Convert --all to explicit --max and --timeout
            CHILD_ARGS+=(--max="$MAX_TESTS" --timeout="$TIMEOUT")
            ;;
        *)
            # Pass through other arguments
            CHILD_ARGS+=("$arg")
            ;;
    esac
done

# Branch based on mode
if [ "$USE_SANDBOX" = false ]; then
    # No sandbox - run native binary directly
    exec "$SCRIPT_DIR/run-native-unsafe.sh" "${CHILD_ARGS[@]}"
else
    # Docker mode - pass wasm/native choice
    exec "$SCRIPT_DIR/run-docker.sh" "--wasm=$USE_WASM" "${CHILD_ARGS[@]}"
fi


