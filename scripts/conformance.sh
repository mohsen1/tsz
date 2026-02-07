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
CACHE_GEN_BIN="$REPO_ROOT/.target/release/generate-tsc-cache"
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
  download    Download TSC cache from GitHub artifacts (fastest)
  generate    Generate TSC cache locally (required if download unavailable)
  run         Run conformance tests against TSC cache
  all         Download/generate cache and run tests (default)
  clean       Remove cache file

Options:
  --workers N       Number of parallel workers (default: 16)
  --max N           Maximum number of tests to run (default: all)
  --verbose         Show per-test results
  --filter PAT      Filter test files by pattern
  --error-code N    Only show tests with this error code (e.g., 2304)
  --no-cache        Force cache regeneration even if cache exists
  --no-download     Skip trying to download cache from GitHub

Examples:
  ./scripts/conformance.sh all                        # Download cache (or generate) and run
  ./scripts/conformance.sh download                   # Download cache from GitHub artifacts
  ./scripts/conformance.sh run --max 100              # Test first 100 files
  ./scripts/conformance.sh run --filter "strict"      # Run tests matching "strict"
  ./scripts/conformance.sh run --error-code 2304      # Only show tests with TS2304
  ./scripts/conformance.sh generate --workers 32      # Regenerate cache with 32 workers
  ./scripts/conformance.sh generate --no-cache        # Force regenerate cache

Note: Binaries are automatically built if not found.
      Cache is downloaded from GitHub artifacts when available (per TypeScript version).
      Use 'generate' to create cache locally if download fails.

Cache location: tsc-cache-full.json (in repo root)
Test directory: TypeScript/tests/cases/conformance
EOF
}

# Build binaries (always rebuilds to pick up code changes; cargo no-ops if unchanged)
ensure_binaries() {
    echo -e "${YELLOW}Building tsz...${NC}"
    cd "$REPO_ROOT"
    cargo build --release -p tsz-cli --bin tsz
    echo ""

    echo -e "${YELLOW}Building conformance runner...${NC}"
    cd "$REPO_ROOT/crates/conformance"
    cargo build --release
    cd "$REPO_ROOT"
    echo ""
}

download_cache() {
    local force="${1:-false}"
    
    if [ "$force" != "true" ] && [ -f "$CACHE_FILE" ]; then
        echo -e "${YELLOW}Cache already exists: $CACHE_FILE${NC}"
        return 0
    fi

    echo -e "${GREEN}Attempting to download TSC cache from GitHub...${NC}"
    
    if [ -x "$REPO_ROOT/scripts/download-tsc-cache.sh" ]; then
        if [ "$force" = "true" ]; then
            "$REPO_ROOT/scripts/download-tsc-cache.sh" --force && return 0
        else
            "$REPO_ROOT/scripts/download-tsc-cache.sh" && return 0
        fi
    fi
    
    echo -e "${YELLOW}Download failed or unavailable${NC}"
    return 1
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

    # Use fast Node.js generator if TypeScript is available
    local FAST_GEN="$REPO_ROOT/scripts/generate-tsc-cache-fast.mjs"
    local TS_MODULE="$REPO_ROOT/scripts/emit/node_modules/typescript/lib/typescript.js"
    
    if [ -f "$FAST_GEN" ] && [ -f "$TS_MODULE" ]; then
        # Cap workers at 8 for the fast generator to avoid OOM
        # (each worker loads a full TypeScript instance ~300-500MB)
        local FAST_WORKERS=$WORKERS
        if [ "$FAST_WORKERS" -gt 8 ]; then
            FAST_WORKERS=8
        fi
        echo -e "${GREEN}Generating TSC cache (fast mode - TypeScript API)...${NC}"
        echo "Test directory: $TEST_DIR"
        echo "Workers: $FAST_WORKERS"
        echo ""
        
        cd "$REPO_ROOT"
        node "$FAST_GEN" \
            --test-dir "$TEST_DIR" \
            --output "$CACHE_FILE" \
            --workers "$FAST_WORKERS" \
            --ts-path "$TS_MODULE"
    else
        echo -e "${GREEN}Generating TSC cache (using tsc directly)...${NC}"
        echo "Test directory: $TEST_DIR"
        echo "Workers: $WORKERS"
        echo ""
        echo -e "${YELLOW}Tip: Install TypeScript in scripts/emit for ~3x faster generation${NC}"
        echo -e "${YELLOW}  cd scripts/emit && npm install${NC}"
        echo ""

        cd "$REPO_ROOT"
        $CACHE_GEN_BIN \
            --test-dir "$TEST_DIR" \
            --output "$CACHE_FILE" \
            --workers "$WORKERS"
    fi

    echo ""
    echo -e "${GREEN}Cache generated: $CACHE_FILE${NC}"
}

# Ensure cache exists - try download first, then generate
ensure_cache() {
    local no_download="${1:-false}"
    
    if [ -f "$CACHE_FILE" ]; then
        return 0
    fi
    
    # Try downloading first (faster)
    if [ "$no_download" != "true" ]; then
        if download_cache; then
            return 0
        fi
        echo ""
    fi
    
    # Fall back to generation
    echo -e "${YELLOW}Generating cache locally (this may take 10-15 minutes)...${NC}"
    ensure_binaries
    generate_cache
}

run_tests() {
    echo -e "${GREEN}Running conformance tests...${NC}"
    echo "Cache file: $CACHE_FILE"
    echo "Workers: $WORKERS"
    echo ""

    cd "$REPO_ROOT"
    # Filter out --workers and --no-cache from passed args to avoid duplication
    local extra_args=()
    local verbose=false
    local has_error_code=false
    local has_max=false
    local prev_arg=""
    for arg in "$@"; do
        if [[ "$arg" == --workers* ]]; then
            # Skip --workers argument (we use our own)
            prev_arg=""
            continue
        fi
        if [ "$arg" = "--no-cache" ]; then
            # Skip --no-cache (already handled)
            prev_arg=""
            continue
        fi
        if [[ "$arg" == --verbose ]]; then
            verbose=true
        fi
        # Check for --error-code (either --error-code N or --error-code=N)
        if [[ "$arg" == --error-code* ]] || [ "$prev_arg" = "--error-code" ]; then
            has_error_code=true
        fi
        # Check for --max (either --max N or --max=N)
        if [[ "$arg" == --max* ]] || [ "$prev_arg" = "--max" ]; then
            has_max=true
        fi
        prev_arg="$arg"
        extra_args+=("$arg")
    done

    # If --verbose, also add --print-test for per-test output
    if [ "$verbose" = true ]; then
        extra_args+=(--print-test)
    fi

    # Show summary with failing test contents when --error-code, --max, or --verbose is used
    local show_summary=false
    if [ "$has_error_code" = true ] || [ "$has_max" = true ] || [ "$verbose" = true ]; then
        show_summary=true
    fi

    if [ "$show_summary" = true ]; then
        # Stream output in real-time AND capture for post-summary
        local tmpfile
        tmpfile=$(mktemp)
        trap "rm -f '$tmpfile'" EXIT

        local runner_exit=0
        $RUNNER_BIN \
            --test-dir "$TEST_DIR" \
            --cache-file "$CACHE_FILE" \
            --tsz-binary "$TSZ_BIN" \
            --workers $WORKERS \
            "${extra_args[@]}" 2>&1 | tee "$tmpfile" || runner_exit=$?

        local output
        output=$(cat "$tmpfile")

        # Extract failing test paths (up to 10) from captured output
        local failing_tests=()
        while IFS= read -r line; do
            if [[ "$line" =~ ^FAIL[[:space:]]+(.+) ]]; then
                local rel_path="${BASH_REMATCH[1]}"
                local test_path="$REPO_ROOT/$rel_path"
                if [ -f "$test_path" ]; then
                    failing_tests+=("$test_path")
                    if [ ${#failing_tests[@]} -ge 10 ]; then
                        break
                    fi
                fi
            fi
        done <<< "$output"

        # Print test file contents after results
        if [ ${#failing_tests[@]} -gt 0 ]; then
            echo ""
            echo -e "${YELLOW}════════════════════════════════════════════════════════════${NC}"
            echo -e "${YELLOW}Test File Contents (${#failing_tests[@]} failing tests)${NC}"
            echo -e "${YELLOW}════════════════════════════════════════════════════════════${NC}"
            echo ""

            for test_file in "${failing_tests[@]}"; do
                local rel_path="${test_file#$REPO_ROOT/}"
                echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
                echo -e "${GREEN}File: $rel_path${NC}"
                echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
                cat "$test_file"
                echo ""
            done

            echo -e "${YELLOW}════════════════════════════════════════════════════════════${NC}"
        fi

        # Re-print summary line(s) last so they're easy to find
        local summary_lines
        summary_lines=$(grep -E '(^(Total|Pass rate|Passed|Failed|Skipped)|passed.*failed|conformance)' "$tmpfile" 2>/dev/null || true)
        if [ -n "$summary_lines" ]; then
            echo ""
            echo -e "${GREEN}═══ Summary ═══${NC}"
            echo "$summary_lines"
        fi

        rm -f "$tmpfile"
    else
        # No summary mode, run normally
        $RUNNER_BIN \
            --test-dir "$TEST_DIR" \
            --cache-file "$CACHE_FILE" \
            --tsz-binary "$TSZ_BIN" \
            --workers $WORKERS \
            "${extra_args[@]}"
    fi

    echo ""
    echo -e "${GREEN}Tests completed${NC}"
}

clean_cache() {
    echo "Removing cache file: $CACHE_FILE"
    rm -f "$CACHE_FILE"
    echo -e "${GREEN}Cache cleaned${NC}"
}

# Parse arguments
# Check for help flags first
if [[ "${1:-}" == "help" ]] || [[ "${1:-}" == "--help" ]] || [[ "${1:-}" == "-h" ]]; then
    COMMAND="help"
    shift || true
# If first argument starts with --, assume user meant 'run' command
elif [[ "${1:-}" == --* ]]; then
    COMMAND="run"
else
    COMMAND="${1:-all}"
    shift || true
fi

# Check for flags
NO_CACHE=false
NO_DOWNLOAD=false
REMAINING_ARGS=()
for arg in "$@"; do
    if [ "$arg" = "--no-cache" ]; then
        NO_CACHE=true
    elif [ "$arg" = "--no-download" ]; then
        NO_DOWNLOAD=true
    else
        REMAINING_ARGS+=("$arg")
    fi
done

case "$COMMAND" in
    download)
        if [ "$NO_CACHE" = "true" ]; then
            download_cache "true"
        else
            download_cache
        fi
        ;;
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
        if [ "$NO_CACHE" = "true" ]; then
            echo -e "${YELLOW}--no-cache flag set, regenerating cache...${NC}"
            generate_cache "true"
            echo ""
        elif [ ! -f "$CACHE_FILE" ]; then
            ensure_cache "$NO_DOWNLOAD"
            echo ""
        fi
        run_tests "${REMAINING_ARGS[@]}"
        ;;
    all)
        ensure_binaries
        if [ "$NO_CACHE" = "true" ]; then
            generate_cache "true"
        else
            ensure_cache "$NO_DOWNLOAD"
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
