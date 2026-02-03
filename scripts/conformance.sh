#!/bin/bash
# Conformance Test Runner
# Usage: ./scripts/conformance.sh [generate|run|all] [options]

set -e

# Get the repository root directory
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Default values (relative to repo root)
TEST_DIR="$REPO_ROOT/TypeScript/tests/cases"
CACHE_FILE="$REPO_ROOT/tsc-cache-full.json"
TSZ_BIN="$REPO_ROOT/.target/release/tsz"
CACHE_GEN_BIN="$REPO_ROOT/.target/release/generate-tsc-cache-tsserver"
RUNNER_BIN="$REPO_ROOT/.target/release/tsz-conformance"
WORKERS=16

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

print_help() {
    cat << EOF
${YELLOW}TSZ Conformance Test Runner${NC}

Usage: ./scripts/conformance.sh [COMMAND] [OPTIONS]

Commands:
  generate    Generate TSC cache (required before first run)
  run         Run conformance tests against TSC cache
  all         Generate cache and run tests (default)
  clean       Remove cache file

Options:
  --workers N       Number of parallel workers (default: 16)
  --max N           Maximum number of tests to run (default: all)
  --verbose         Show per-test results
  --filter PAT      Filter test files by pattern
  --error-code N    Only show tests with this error code (e.g., 2304)
  --no-cache        Force cache regeneration even if cache exists

Examples:
  ./scripts/conformance.sh all                        # Full pipeline
  ./scripts/conformance.sh run --max 100              # Test first 100 files
  ./scripts/conformance.sh run --filter "strict"      # Run tests matching "strict"
  ./scripts/conformance.sh run --error-code 2304      # Only show tests with TS2304
  ./scripts/conformance.sh generate --workers 32      # Regenerate cache with 32 workers
  ./scripts/conformance.sh generate --no-cache       # Force regenerate cache

Note: Binaries are automatically built if not found.
Cache location: tsc-cache-full.json (in repo root)
Test directory: TypeScript/tests/cases/conformance
EOF
}

# Build binaries if needed
ensure_binaries() {
    local need_tsz=false
    local need_conformance=false

    if [ ! -f "$TSZ_BIN" ]; then
        need_tsz=true
    fi

    if [ ! -f "$CACHE_GEN_BIN" ] || [ ! -f "$RUNNER_BIN" ]; then
        need_conformance=true
    fi

    if [ "$need_tsz" = true ]; then
        echo -e "${YELLOW}Building tsz...${NC}"
        cd "$REPO_ROOT"
        cargo build --release --bin tsz
        echo ""
    fi

    if [ "$need_conformance" = true ]; then
        echo -e "${YELLOW}Building conformance runner...${NC}"
        cd "$REPO_ROOT/conformance-rust"
        cargo build --release
        cd "$REPO_ROOT"
        echo ""
    fi

    # Final check
    if [ ! -f "$TSZ_BIN" ]; then
        echo "Error: tsz binary not found at $TSZ_BIN after build"
        exit 1
    fi

    if [ ! -f "$CACHE_GEN_BIN" ]; then
        echo "Error: generate-tsc-cache binary not found at $CACHE_GEN_BIN after build"
        exit 1
    fi

    if [ ! -f "$RUNNER_BIN" ]; then
        echo "Error: tsz-conformance binary not found at $RUNNER_BIN after build"
        exit 1
    fi
}

generate_cache() {
    local force_regenerate="${1:-false}"
    
    if [ "$force_regenerate" != "true" ] && [ -f "$CACHE_FILE" ]; then
        echo -e "${YELLOW}Cache already exists: $CACHE_FILE${NC}"
        echo "Skipping cache generation."
        echo ""
        return
    fi

    if [ "$force_regenerate" = "true" ] && [ -f "$CACHE_FILE" ]; then
        echo -e "${YELLOW}Cache exists but --no-cache flag set, regenerating...${NC}"
        echo ""
    fi

    echo -e "${GREEN}Generating TSC cache (using tsserver)...${NC}"
    echo "Test directory: $TEST_DIR"
    echo ""

    cd "$REPO_ROOT"
    $CACHE_GEN_BIN \
        --test-dir "$TEST_DIR" \
        --output "$CACHE_FILE"

    echo ""
    echo -e "${GREEN}Cache generated: $CACHE_FILE${NC}"
}

run_tests() {
    echo -e "${GREEN}Running conformance tests...${NC}"
    echo "Cache file: $CACHE_FILE"
    echo "Workers: $WORKERS"
    echo ""

    cd "$REPO_ROOT"
    # Filter out --workers and --no-cache from passed args to avoid duplication
    local filtered_args=()
    local extra_args=()
    local verbose=false
    for arg in "$@"; do
        if [[ "$arg" == --workers* ]]; then
            # Skip --workers argument (we use our own)
            continue
        fi
        if [ "$arg" = "--no-cache" ]; then
            # Skip --no-cache (already handled)
            continue
        fi
        if [[ "$arg" == --verbose ]]; then
            verbose=true
        fi
        extra_args+=("$arg")
    done

    # If --verbose, also add --print-test for per-test output
    if [ "$verbose" = true ]; then
        extra_args+=(--print-test)
    fi

    $RUNNER_BIN \
        --test-dir "$TEST_DIR" \
        --cache-file "$CACHE_FILE" \
        --tsz-binary "$TSZ_BIN" \
        --workers $WORKERS \
        "${extra_args[@]}"

    echo ""
    echo -e "${GREEN}Tests completed${NC}"
}

clean_cache() {
    echo "Removing cache file: $CACHE_FILE"
    rm -f "$CACHE_FILE"
    echo -e "${GREEN}Cache cleaned${NC}"
}

# Parse arguments
COMMAND="${1:-all}"
shift || true

# Check for --no-cache flag
NO_CACHE=false
REMAINING_ARGS=()
for arg in "$@"; do
    if [ "$arg" = "--no-cache" ]; then
        NO_CACHE=true
    else
        REMAINING_ARGS+=("$arg")
    fi
done

case "$COMMAND" in
    generate)
        ensure_binaries
        if [ "$NO_CACHE" = "true" ]; then
            generate_cache "true"
        else
            generate_cache
        fi
        ;;
    run)
        ensure_binaries
        if [ "$NO_CACHE" = "true" ] || [ ! -f "$CACHE_FILE" ]; then
            if [ "$NO_CACHE" = "true" ]; then
                echo -e "${YELLOW}--no-cache flag set, regenerating cache...${NC}"
            else
                echo -e "${YELLOW}Cache not found, generating first...${NC}"
            fi
            echo ""
            if [ "$NO_CACHE" = "true" ]; then
                generate_cache "true"
            else
                generate_cache
            fi
            echo ""
        fi
        run_tests "${REMAINING_ARGS[@]}"
        ;;
    all)
        ensure_binaries
        if [ "$NO_CACHE" = "true" ]; then
            generate_cache "true"
        else
            generate_cache
        fi
        echo ""
        run_tests "${REMAINING_ARGS[@]}"
        ;;
    clean)
        clean_cache
        ;;
    help|--help|-h)
        print_help
        ;;
    *)
        echo "Error: Unknown command '$COMMAND'"
        echo ""
        print_help
        exit 1
        ;;
esac
