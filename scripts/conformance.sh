#!/bin/bash
# Conformance Test Runner
# Usage: ./scripts/conformance.sh [generate|run|all] [options]

set -e

# Get the repository root directory
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Default values (relative to repo root)
TEST_DIR="$REPO_ROOT/TypeScript/tests/cases"
CACHE_FILE="$REPO_ROOT/tsc-cache-full.json"

# Build profile (dist-fast = fast build + good runtime perf)
BUILD_PROFILE="dist-fast"

# Binary paths (will be updated based on profile)
TSZ_BIN="$REPO_ROOT/.target/dist-fast/tsz"
CACHE_GEN_BIN="$REPO_ROOT/.target/dist-fast/generate-tsc-cache"
RUNNER_BIN="$REPO_ROOT/.target/dist-fast/tsz-conformance"

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
  generate    Generate TSC cache locally (if not checked in)
  run         Run conformance tests against TSC cache
  analyze     Analyze failures: categorize, rank by impact, find easy wins
              Shows which error codes to implement for maximum conformance gain
  all         Generate cache (if needed) and run tests (default)
  clean       Remove cache file

Options:
  --workers N       Number of parallel workers (default: 16)
  --max N           Maximum number of tests to run (default: all)
  --offset N        Skip first N tests (default: 0)
  --verbose         Show per-test results
  --compare-fingerprints  Compare full diagnostic fingerprints (requires regenerated cache)
  --print-fingerprints    Print fingerprint deltas for failed tests
  --filter PAT      Filter test files by pattern
  --error-code N    Only show tests with this error code (e.g., 2304)
  --no-cache        Force cache regeneration even if cache exists
  --profile NAME    Use specific cargo profile (default: dist-fast, available: dist-fast, dist, release, dev)

Analyze options:
  --category CAT    Filter by category: false-positive, all-missing, wrong-code, close
  --top N           Show top N items per section (default: 20)

Examples:
  ./scripts/conformance.sh run --max 100              # Test first 100 files
  ./scripts/conformance.sh run --filter "strict"      # Run tests matching "strict"
  ./scripts/conformance.sh run --error-code 2304      # Only show tests with TS2304
  ./scripts/conformance.sh run --compare-fingerprints --print-fingerprints  # Rich diagnostic parity
  ./scripts/conformance.sh analyze                    # Full analysis with impact report
  ./scripts/conformance.sh analyze --offset 0 --max 3101  # Analyze slice failures
  ./scripts/conformance.sh analyze --category false-positive  # Show only false positives
  ./scripts/conformance.sh analyze --category close    # Tests closest to passing
  ./scripts/conformance.sh analyze --top 30            # Show top 30 items per section

Analysis output includes:
  - Error codes NOT IMPLEMENTED (never emitted by tsz) - highest priority!
  - Error codes that appear TOGETHER in tests - implement groups for more impact
  - QUICK WINS - tests missing just one error code
  - Impact estimation - how many tests each code affects

Note: Binaries are automatically built if not found.
      Cache (tsc-cache-full.json) is checked into the repo.

Cache location: tsc-cache-full.json (in repo root)
Test directory: TypeScript/tests/cases/conformance
EOF
}

# Cross-platform file modification time (seconds since epoch)
# Linux: stat -c %Y, macOS: stat -f %m
file_mtime() {
    if stat -c %Y /dev/null >/dev/null 2>&1; then
        stat -c %Y "$1" 2>/dev/null
    else
        stat -f %m "$1" 2>/dev/null
    fi
}

# Check if binaries are up-to-date with source code
# Returns 0 if binaries are fresh (up-to-date), 1 if they need rebuilding
binaries_are_fresh() {
    local binary_dir="$REPO_ROOT/.target/$BUILD_PROFILE"
    local tsz_bin="$binary_dir/tsz"
    local conformance_bin="$binary_dir/tsz-conformance"
    local cache_gen_bin="$binary_dir/generate-tsc-cache"
    
    # Check if all binaries exist
    if [ ! -f "$tsz_bin" ] || [ ! -f "$conformance_bin" ] || [ ! -f "$cache_gen_bin" ]; then
        return 1
    fi
    
    # Find the newest binary modification time
    local newest_binary_mtime=$(for f in "$tsz_bin" "$conformance_bin" "$cache_gen_bin"; do file_mtime "$f"; done | sort -n | tail -1)
    
    # Check if any Rust source file in the relevant crates is newer than the binaries
    # These are all the workspace crates that tsz-cli and tsz-conformance depend on
    local crates_to_check=(
        "tsz-cli"
        "conformance"
        "tsz-common"
        "tsz-scanner"
        "tsz-parser"
        "tsz-binder"
        "tsz-solver"
        "tsz-checker"
        "tsz-emitter"
        "tsz-lsp"
        "tsz-wasm"
    )
    
    local crates_dir="$REPO_ROOT/crates"
    
    for crate_name in "${crates_to_check[@]}"; do
        local crate_dir="$crates_dir/$crate_name"
        
        # Check source files
        if [ -d "$crate_dir/src" ]; then
            while IFS= read -r -d '' src_file; do
                local src_mtime=$(file_mtime "$src_file")
                if [ "$src_mtime" -gt "$newest_binary_mtime" ]; then
                    return 1
                fi
            done < <(find "$crate_dir/src" -name "*.rs" -print0 2>/dev/null)
        fi
        
        # Check Cargo.toml
        if [ -f "$crate_dir/Cargo.toml" ]; then
            local toml_mtime=$(file_mtime "$crate_dir/Cargo.toml")
            if [ "$toml_mtime" -gt "$newest_binary_mtime" ]; then
                return 1
            fi
        fi
    done
    
    # Check root Cargo.toml and Cargo.lock
    if [ -f "$REPO_ROOT/Cargo.toml" ]; then
        local root_toml_mtime=$(file_mtime "$REPO_ROOT/Cargo.toml")
        if [ "$root_toml_mtime" -gt "$newest_binary_mtime" ]; then
            return 1
        fi
    fi
    if [ -f "$REPO_ROOT/Cargo.lock" ]; then
        local lock_mtime=$(file_mtime "$REPO_ROOT/Cargo.lock")
        if [ "$lock_mtime" -gt "$newest_binary_mtime" ]; then
            return 1
        fi
    fi
    
    return 0
}

# Build binaries (always rebuilds to pick up code changes; cargo no-ops if unchanged)
ensure_binaries() {
    # Fast path: check if binaries are already fresh
    if binaries_are_fresh; then
        echo -e "${GREEN}Binaries are up-to-date (profile: $BUILD_PROFILE)${NC}"
        return 0
    fi
    
    echo -e "${YELLOW}Building tsz and conformance runner (profile: $BUILD_PROFILE)...${NC}"
    cd "$REPO_ROOT"
    
    # For dev profile, optimize for fast build (link time not important)
    # For release/dist, LTO is already configured in Cargo.toml
    # NOTE: On macOS, ThinLTO + incremental can intermittently fail at link-time
    # with undefined llvm internal symbols. Disable incremental for dist profiles
    # in this script to keep conformance runs stable.
    local cargo_incremental="${CARGO_INCREMENTAL:-1}"
    if [[ "$BUILD_PROFILE" == "dist" || "$BUILD_PROFILE" == "dist-fast" ]]; then
        cargo_incremental="0"
    fi
    CARGO_INCREMENTAL="$cargo_incremental" cargo build --profile "$BUILD_PROFILE" -p tsz-cli -p tsz-conformance
    
    echo ""
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

# Ensure cache exists - generate if not checked in
ensure_cache() {
    if [ -f "$CACHE_FILE" ]; then
        return 0
    fi

    echo -e "${YELLOW}Cache not found, generating locally (this may take 10-15 minutes)...${NC}"
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
    local print_fingerprints=false
    local has_error_code=false
    local has_max=false
    local prev_arg=""
    local skip_next=false
    for arg in "$@"; do
        if [ "$skip_next" = true ]; then
            skip_next=false
            continue
        fi
        if [ "$arg" = "--workers" ]; then
            # Skip --workers and its value (we use our own)
            skip_next=true
            prev_arg=""
            continue
        fi
        if [[ "$arg" == --workers=* ]]; then
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
        if [[ "$arg" == --print-fingerprints ]]; then
            print_fingerprints=true
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

    # Always capture expected/actual/config (needed for last 10 failing tests section)
    extra_args+=(--print-test)
    local show_per_test=$verbose
    if [ "$print_fingerprints" = true ]; then
        show_per_test=true
    fi

    # Always capture output to extract failing tests and their details
    local tmpfile
    tmpfile=$(mktemp)
    trap "rm -f '$tmpfile'" EXIT

    local runner_exit=0
    $RUNNER_BIN \
        --test-dir "$TEST_DIR" \
        --cache-file "$CACHE_FILE" \
        --tsz-binary "$TSZ_BIN" \
        --workers $WORKERS \
        "${extra_args[@]}" | tee "$tmpfile" | if [ "$show_per_test" = true ]; then
            # --verbose: show all lines including expected/actual/options
            grep -E '^(PASS|FAIL|SKIP|CRASH|⏱️) |^  (expected|actual|options):|^  (missing-fingerprints|extra-fingerprints):|^    - ' 2>/dev/null || true
        else
            # default: only show FAIL/CRASH/TIMEOUT file names (no expected/actual/options)
            grep -E '^(FAIL|CRASH|⏱️) ' 2>/dev/null || true
        fi || runner_exit=$?

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

    # Print test file contents with expected/actual/config for first 10 failing tests
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
            
            # Extract expected, actual, and config values for this test
            local expected=""
            local actual=""
            local config=""
            local in_test_block=false
            while IFS= read -r line; do
                # Match FAIL line with this test path (may have additional text after path)
                if [[ "$line" =~ ^FAIL[[:space:]]+$rel_path(.*)$ ]]; then
                    in_test_block=true
                    continue
                fi
                if [ "$in_test_block" = true ]; then
                    # Stop at next test result line
                    if [[ "$line" =~ ^FAIL[[:space:]]+ ]] || [[ "$line" =~ ^PASS[[:space:]]+ ]] || [[ "$line" =~ ^SKIP[[:space:]]+ ]]; then
                        break
                    fi
                    # Extract expected/actual/options values (indented with 2 spaces)
                    if [[ "$line" =~ ^[[:space:]][[:space:]]expected:[[:space:]]+(.+) ]]; then
                        expected="${BASH_REMATCH[1]}"
                    elif [[ "$line" =~ ^[[:space:]][[:space:]]actual:[[:space:]]+(.+) ]]; then
                        actual="${BASH_REMATCH[1]}"
                    elif [[ "$line" =~ ^[[:space:]][[:space:]]options:[[:space:]]+(.+) ]]; then
                        config="${BASH_REMATCH[1]}"
                    fi
                fi
            done <<< "$output"
            
            # Print verbose expected, actual, and config values
            if [ -n "$expected" ] || [ -n "$actual" ] || [ -n "$config" ]; then
                echo ""
                if [ -n "$expected" ]; then
                    echo -e "  ${YELLOW}expected:${NC} $expected"
                fi
                if [ -n "$actual" ]; then
                    echo -e "  ${YELLOW}actual:${NC} $actual"
                fi
                if [ -n "$config" ]; then
                    echo -e "  ${YELLOW}config:${NC} $config"
                fi
            fi
            
            echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
            cat "$test_file"
            echo ""
        done

        echo -e "${YELLOW}════════════════════════════════════════════════════════════${NC}"
    fi

    # Print final results block LAST
    echo ""
    sed -n '/^=\{10,\}/,/^=\{10,\}/p' "$tmpfile" 2>/dev/null || true

    rm -f "$tmpfile"

    echo ""
    echo -e "${GREEN}Tests completed${NC}"
}

analyze_tests() {
    local category_filter=""
    local top_n=20
    local extra_args=()

    # Parse analyze-specific args
    local args=("$@")
    local i=0
    while [ $i -lt ${#args[@]} ]; do
        case "${args[$i]}" in
            --category)
                i=$((i + 1))
                category_filter="${args[$i]}"
                ;;
            --top)
                i=$((i + 1))
                top_n="${args[$i]}"
                ;;
            *)
                extra_args+=("${args[$i]}")
                ;;
        esac
        i=$((i + 1))
    done

    echo -e "${GREEN}Running conformance tests for analysis...${NC}"

    cd "$REPO_ROOT"

    # Run with --print-test to get expected/actual per test
    local tmpfile
    tmpfile=$(mktemp)
    trap "rm -f '$tmpfile'" EXIT

    $RUNNER_BIN \
        --test-dir "$TEST_DIR" \
        --cache-file "$CACHE_FILE" \
        --tsz-binary "$TSZ_BIN" \
        --workers $WORKERS \
        --print-test \
        "${extra_args[@]}" > "$tmpfile" 2>/dev/null || true

    # Use python to analyze the output
    python3 "$REPO_ROOT/scripts/analyze-conformance.py" "$tmpfile" "$category_filter" "$top_n"
}

clean_cache() {
    echo "Removing cache file: $CACHE_FILE"
    rm -f "$CACHE_FILE"
    echo -e "${GREEN}Cache cleaned${NC}"
}

# Ensure the TypeScript submodule is pristine before running tests.
# tsc can emit .d.ts/.js files next to test files, polluting the submodule
# and causing cache misses (extra .js files get picked up as test inputs).
# Always run git clean -xf to guarantee a clean state.
check_submodule_clean() {
    local ts_dir="$REPO_ROOT/TypeScript"
    if [ ! -d "$ts_dir/.git" ] && [ ! -f "$ts_dir/.git" ]; then
        return 0  # Not a git repo/submodule, skip check
    fi

    # Verify the submodule SHA matches what's committed in the parent repo.
    # This catches accidental `cd TypeScript && git checkout <other>` or detached HEAD drift.
    local expected_sha
    expected_sha=$(cd "$REPO_ROOT" && git ls-tree HEAD TypeScript 2>/dev/null | awk '{print $3}')

    # Prefer repository pinned TypeScript SHA so local workflow can proceed with
    # the intended submodule version tracked in scripts/typescript-versions.json
    # even before the superproject commit is updated.
    local pinned_sha
    pinned_sha=$(node -e "const fs = require('fs'); const p = 'scripts/typescript-versions.json'; try { const v = JSON.parse(fs.readFileSync(p, 'utf8')); process.stdout.write(v.current || ''); } catch {}" | tr -d '\n')
    if [ -n "$pinned_sha" ]; then
        expected_sha="$pinned_sha"
    fi
    local actual_sha
    actual_sha=$(cd "$ts_dir" && git rev-parse HEAD 2>/dev/null)

    if [ -n "$expected_sha" ] && [ -n "$actual_sha" ] && [ "$expected_sha" != "$actual_sha" ]; then
        echo -e "${YELLOW}⚠ TypeScript submodule SHA mismatch!${NC}"
        echo "  Expected (committed): $expected_sha"
        echo "  Actual (checked out): $actual_sha"
        echo -e "${YELLOW}Resetting to committed SHA...${NC}"
        (cd "$REPO_ROOT" && git submodule update --init TypeScript 2>/dev/null)
    fi

    echo -e "${YELLOW}Cleaning TypeScript submodule (git checkout + clean -xfd)...${NC}"
    (cd "$ts_dir" && git checkout -- . 2>/dev/null; git clean -xfd 2>/dev/null)
    echo -e "${GREEN}✓ TypeScript submodule clean${NC}"
    echo ""
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
REMAINING_ARGS=()
i=0
while [ $i -lt ${#@} ]; do
    arg="${@:$((i+1)):1}"
    if [ "$arg" = "--no-cache" ]; then
        NO_CACHE=true
    elif [ "$arg" = "--no-download" ]; then
        # Kept for backward compatibility, now a no-op
        true
    elif [ "$arg" = "--workers" ]; then
        i=$((i + 1))
        WORKERS="${@:$((i+1)):1}"
    elif [ "$arg" = "--profile" ]; then
        i=$((i + 1))
        BUILD_PROFILE="${@:$((i+1)):1}"
        TSZ_BIN="$REPO_ROOT/.target/$BUILD_PROFILE/tsz"
        CACHE_GEN_BIN="$REPO_ROOT/.target/$BUILD_PROFILE/generate-tsc-cache"
        RUNNER_BIN="$REPO_ROOT/.target/$BUILD_PROFILE/tsz-conformance"
    else
        REMAINING_ARGS+=("$arg")
    fi
    i=$((i + 1))
done

case "$COMMAND" in
    generate)
        check_submodule_clean
        ensure_binaries
        if [ "$NO_CACHE" = "true" ]; then
            generate_cache "true"
        else
            generate_cache
        fi
        ;;
    run)
        check_submodule_clean
        ensure_binaries
        if [ "$NO_CACHE" = "true" ]; then
            echo -e "${YELLOW}--no-cache flag set, regenerating cache...${NC}"
            generate_cache "true"
            echo ""
        elif [ ! -f "$CACHE_FILE" ]; then
            ensure_cache
            echo ""
        fi
        run_tests "${REMAINING_ARGS[@]}"
        ;;
    analyze)
        check_submodule_clean
        ensure_binaries
        if [ ! -f "$CACHE_FILE" ]; then
            ensure_cache
            echo ""
        fi
        analyze_tests "${REMAINING_ARGS[@]}"
        ;;
    all)
        check_submodule_clean
        ensure_binaries
        if [ "$NO_CACHE" = "true" ]; then
            generate_cache "true"
        else
            ensure_cache
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
