#!/usr/bin/env bash
#
# TSZ Conformance Test Runner
# ===========================
#
# Usage: ./run.sh [command] [options]
#
# Examples:
#   ./run.sh                          # Run with defaults (server mode, all tests)
#   ./run.sh --max=100 --verbose      # Run 100 tests with verbose output
#   ./run.sh cache generate           # Generate TSC cache for faster runs
#   ./run.sh single path/to/test.ts   # Run a single test file
#
# For full help: ./run.sh --help

set -euo pipefail

# =============================================================================
# Signal Handling
# =============================================================================

CHILD_PIDS=()

cleanup() {
    local exit_code=$?
    echo ""
    for pid in "${CHILD_PIDS[@]:-}"; do
        kill -TERM "$pid" 2>/dev/null || true
    done
    pkill -P $$ 2>/dev/null || true
    exit $exit_code
}

trap cleanup INT TERM EXIT

# =============================================================================
# Paths & Colors
# =============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

if [[ -t 1 ]]; then
    RED='\033[0;31m' GREEN='\033[0;32m' YELLOW='\033[0;33m'
    BLUE='\033[0;34m' CYAN='\033[0;36m' BOLD='\033[1m'
    DIM='\033[2m' RESET='\033[0m'
else
    RED='' GREEN='' YELLOW='' BLUE='' CYAN='' BOLD='' DIM='' RESET=''
fi

# =============================================================================
# Global Config (set by parse_args)
# =============================================================================

CFG_MODE="server"
CFG_MAX=99999
CFG_WORKERS=""
CFG_TIMEOUT=600
CFG_CATEGORIES="conformance,compiler"
CFG_VERBOSE=false
CFG_FILTER=""
CFG_PRINT_TEST=false
CFG_DUMP_RESULTS=""
CFG_PASS_RATE_ONLY=false
CFG_DRY_RUN=false

# =============================================================================
# Logging & Utilities
# =============================================================================

log_info()    { echo -e "${BLUE}ℹ${RESET}  $*"; }
log_success() { echo -e "${GREEN}✓${RESET}  $*"; }
log_warning() { echo -e "${YELLOW}⚠${RESET}  $*"; }
log_error()   { echo -e "${RED}✗${RESET}  $*" >&2; }
log_step()    { echo -e "${CYAN}→${RESET}  $*"; }

die() { log_error "$@"; exit 2; }

detect_cores() {
    if [[ "$OSTYPE" == "darwin"* ]]; then
        sysctl -n hw.ncpu
    elif [[ -f /proc/cpuinfo ]]; then
        nproc
    else
        echo 4
    fi
}

require_cmd() {
    command -v "$1" &>/dev/null || die "Required command not found: $1"
}

get_target_dir() {
    local config_file="$ROOT_DIR/.cargo/config.toml"
    if [[ -f "$config_file" ]]; then
        local dir
        dir=$(grep -E '^target-dir' "$config_file" 2>/dev/null | awk -F'"' '{print $2}')
        if [[ -n "$dir" ]]; then
            echo "$ROOT_DIR/$dir"
            return
        fi
    fi
    echo "$ROOT_DIR/target"
}

# =============================================================================
# Build Functions
# =============================================================================

build_wasm() {
    log_step "Building WASM module..."
    require_cmd wasm-pack
    (cd "$ROOT_DIR" && wasm-pack build --target nodejs --out-dir pkg --release)
    log_success "WASM module built"
}

build_native() {
    log_step "Building native binary (dev-fast)..."
    require_cmd cargo
    (cd "$ROOT_DIR" && cargo build --profile dev-fast --bin tsz)
    log_success "Native binary built"
}

build_server() {
    log_step "Building tsz-server (dev-fast)..."
    require_cmd cargo
    (cd "$ROOT_DIR" && cargo build --profile dev-fast --bin tsz-server)
    log_success "tsz-server built"
}

build_runner() {
    log_step "Building conformance runner..."
    (
        cd "$SCRIPT_DIR"
        if [[ ! -d "node_modules" ]] || [[ ! -d "node_modules/typescript" ]]; then
            npm install --silent 2>/dev/null || npm install
        fi
        npm run build --silent 2>/dev/null || npm run build
    )
    log_success "Runner built"
}

# =============================================================================
# Output
# =============================================================================

print_banner() {
    local mode_desc="$1"
    echo ""
    echo -e "${CYAN}╔══════════════════════════════════════════════════════════════╗${RESET}"
    echo -e "${CYAN}║${RESET}${BOLD}         TSZ Conformance Test Runner                          ${RESET}${CYAN}║${RESET}"
    echo -e "${CYAN}╠══════════════════════════════════════════════════════════════╣${RESET}"
    echo -e "${CYAN}║${RESET}  Mode:       $(printf '%-48s' "$mode_desc")${CYAN}║${RESET}"
    echo -e "${CYAN}║${RESET}  Tests:      $(printf '%-48s' "$CFG_MAX")${CYAN}║${RESET}"
    echo -e "${CYAN}║${RESET}  Workers:    $(printf '%-48s' "$CFG_WORKERS")${CYAN}║${RESET}"
    echo -e "${CYAN}║${RESET}  Categories: $(printf '%-48s' "$CFG_CATEGORIES")${CYAN}║${RESET}"
    echo -e "${CYAN}║${RESET}  Timeout:    $(printf '%-48s' "${CFG_TIMEOUT}s")${CYAN}║${RESET}"
    if [[ -n "$CFG_FILTER" ]]; then
        echo -e "${CYAN}║${RESET}  Filter:     $(printf '%-48s' "$CFG_FILTER")${CYAN}║${RESET}"
    fi
    echo -e "${CYAN}╚══════════════════════════════════════════════════════════════╝${RESET}"
    echo ""
}

# Run node runner with optional timeout, capturing exit code correctly.
run_node() {
    local exit_code=0
    if command -v timeout &>/dev/null; then
        timeout "${CFG_TIMEOUT}s" node --expose-gc dist/runner.js "$@" || {
            exit_code=$?
            if [[ $exit_code -eq 124 ]]; then
                echo ""
                log_warning "Tests timed out after ${CFG_TIMEOUT}s"
            fi
        }
    else
        node --expose-gc dist/runner.js "$@" || exit_code=$?
    fi
    return $exit_code
}

# =============================================================================
# Help
# =============================================================================

show_help() {
    cat << 'EOF'
TSZ Conformance Test Runner

USAGE:
    ./run.sh [command] [options]

COMMANDS:
    (default)       Run conformance tests
    cache           Manage TSC result cache (generate | status | clear)
    single <file>   Run a single test file with detailed output
    help            Show this help message

OPTIONS:
  Execution Mode:
    --server            Persistent tsz-server process (default, fastest)
    --wasm              WASM module (good isolation via WASM sandbox)
    --native            Native binary per test (legacy)

  Test Selection:
    --all               Run all tests (default)
    --max=N             Maximum number of tests
    --category=CAT      Categories: conformance,compiler,projects (default: conformance,compiler)
    --filter=PATTERN    Only run tests matching pattern

  Execution:
    --workers=N         Parallel workers (default: auto-detect, capped at 8)
    --timeout=SECS      Timeout in seconds (default: 600)

  Output:
    -v, --verbose       Detailed output per test
    --print-test        Show file content, directives, expected vs actual (use with --filter)
    --pass-rate-only    Output only pass rate percentage (for CI/scripts)
    -q, --quiet         Minimal output
    --dump-results=FILE Dump full results to JSON file

  Other:
    -h, --help          Show this help
    --version           Show version info
    --dry-run           Show config without running

EXECUTION MODES:
    Server (default)    Persistent process, libs cached in memory, fastest
    WASM                WASM sandbox provides isolation from crashes/OOM
    Native (legacy)     Spawns binary per test, no isolation

EXIT CODES:
    0    All tests passed
    1    Some tests failed
    2    Configuration error
    124  Timeout exceeded
EOF
}

show_version() {
    echo "TSZ Conformance Test Runner v1.0.0"
    echo "Rust: $(rustc --version 2>/dev/null || echo 'not installed')"
    echo "Node: $(node --version 2>/dev/null || echo 'not installed')"
}

# =============================================================================
# Commands: cache, single
# =============================================================================

cmd_cache() {
    local subcmd="${1:-status}"
    build_runner
    case "$subcmd" in
        generate) log_step "Generating TSC cache..."; (cd "$SCRIPT_DIR" && node dist/generate-cache.js) ;;
        status)   (cd "$SCRIPT_DIR" && node dist/generate-cache.js --status) ;;
        clear)    log_step "Clearing TSC cache..."; (cd "$SCRIPT_DIR" && node dist/generate-cache.js --clear); log_success "Cache cleared" ;;
        *)        die "Unknown cache command: $subcmd (use: generate, status, clear)" ;;
    esac
}

cmd_single() {
    local test_file="${1:-}"
    [[ -z "$test_file" ]] && die "Usage: ./run.sh single <path/to/test.ts>"

    # Resolve path
    if [[ ! -f "$test_file" ]] && [[ -f "$ROOT_DIR/$test_file" ]]; then
        test_file="$ROOT_DIR/$test_file"
    elif [[ ! -f "$test_file" ]]; then
        die "Test file not found: $test_file"
    fi
    [[ "$test_file" != /* ]] && test_file="$(pwd)/$test_file"

    build_native

    local binary
    binary="$(get_target_dir)/dev-fast/tsz"

    echo ""
    echo -e "${BOLD}Running: ${RESET}$test_file"
    echo "─────────────────────────────────────────────────────────────"
    echo ""
    (cd "$ROOT_DIR" && "$binary" "$test_file" 2>&1) || true
    echo ""
    echo "─────────────────────────────────────────────────────────────"
}

# =============================================================================
# Run Modes
# =============================================================================

# Build runner args from global config.
build_runner_args() {
    local args="--max=$CFG_MAX --workers=$CFG_WORKERS --category=$CFG_CATEGORIES"
    [[ "$CFG_VERBOSE" == "true" ]]    && args="$args --verbose"
    [[ -n "$CFG_FILTER" ]]            && args="$args --filter=$CFG_FILTER"
    [[ "$CFG_PRINT_TEST" == "true" ]] && args="$args --print-test"
    [[ -n "$CFG_DUMP_RESULTS" ]]      && args="$args --dump-results=$CFG_DUMP_RESULTS"
    echo "$args"
}

run_server_mode() {
    [[ "$CFG_PRINT_TEST" != "true" ]] && print_banner "Server (persistent)"

    build_server
    build_runner

    [[ "$CFG_PRINT_TEST" != "true" ]] && { log_step "Starting tsz-server pool..."; echo ""; }

    cd "$SCRIPT_DIR"
    local target_dir
    target_dir=$(get_target_dir)
    export TSZ_SERVER_BINARY="$target_dir/dev-fast/tsz-server"
    export TSZ_LIB_DIR="$ROOT_DIR/TypeScript/src/lib"

    local args
    args="$(build_runner_args) --server"

    local exit_code=0
    run_node $args || exit_code=$?

    if [[ "$CFG_PRINT_TEST" != "true" ]]; then
        echo ""
        echo -e "${DIM}Tip: Use --filter=PATTERN --print-test to see detailed info for specific test failures${RESET}"
    fi

    return $exit_code
}

run_wasm_native_mode() {
    local use_wasm="$1"
    local mode_desc
    if [[ "$use_wasm" == "true" ]]; then mode_desc="WASM"; else mode_desc="Native"; fi

    print_banner "$mode_desc"

    if [[ "$use_wasm" == "true" ]]; then
        build_wasm
    else
        build_native
    fi
    build_runner

    log_step "Running tests..."
    echo ""
    cd "$SCRIPT_DIR"

    if [[ "$use_wasm" == "false" ]]; then
        export TSZ_BINARY="$(get_target_dir)/dev-fast/tsz"
    fi

    local runner_args
    runner_args="$(build_runner_args) --wasm=$use_wasm"
    run_node $runner_args
}

# Dispatch to the appropriate run mode.
run_mode() {
    case "$CFG_MODE" in
        server) run_server_mode ;;
        wasm)   run_wasm_native_mode true ;;
        native) run_wasm_native_mode false ;;
    esac
}

# =============================================================================
# Argument Parsing
# =============================================================================

parse_args() {
    local command=""
    local positional_args=()

    while [[ $# -gt 0 ]]; do
        case "$1" in
            help|--help|-h)    show_help; exit 0 ;;
            --version)         show_version; exit 0 ;;
            cache)             command="cache"; shift; positional_args+=("$@"); break ;;
            single)            command="single"; shift; positional_args+=("$@"); break ;;

            --server)          CFG_MODE="server" ;;
            --wasm)            CFG_MODE="wasm" ;;
            --native)          CFG_MODE="native" ;;

            --all)             CFG_MAX=99999; CFG_TIMEOUT=3600 ;;
            --max=*)           CFG_MAX="${1#*=}" ;;
            --category=*)      CFG_CATEGORIES="${1#*=}" ;;
            --filter=*)        CFG_FILTER="${1#*=}" ;;
            --print-test)      CFG_PRINT_TEST=true ;;
            --dump-results=*)  CFG_DUMP_RESULTS="${1#*=}" ;;
            --dump-results)    CFG_DUMP_RESULTS="$SCRIPT_DIR/.tsc-cache/test-results.json" ;;
            --pass-rate-only)  CFG_PASS_RATE_ONLY=true ;;

            --workers=*)       CFG_WORKERS="${1#*=}" ;;
            --timeout=*)       CFG_TIMEOUT="${1#*=}" ;;

            -v|--verbose)      CFG_VERBOSE=true ;;
            -q|--quiet)        ;; # TODO
            --json)            log_warning "--json not yet implemented" ;;

            --dry-run)         CFG_DRY_RUN=true ;;

            # Accept but ignore removed Docker flags for backwards compat
            --docker|--no-docker|--no-sandbox) ;;

            -*)                die "Unknown option: $1 (use --help for usage)" ;;
            *)                 positional_args+=("$1") ;;
        esac
        shift
    done

    # Auto-detect workers
    if [[ -z "$CFG_WORKERS" ]]; then
        CFG_WORKERS=$(detect_cores)
        if (( CFG_WORKERS > 8 )) && (( CFG_MAX < 1000 )); then
            CFG_WORKERS=8
        fi
    fi

    # Handle subcommands
    case "$command" in
        cache)  cmd_cache "${positional_args[@]:-status}"; exit $? ;;
        single) cmd_single "${positional_args[@]:-}"; exit $? ;;
    esac
}

# =============================================================================
# Main
# =============================================================================

main() {
    parse_args "$@"

    if [[ "$CFG_DRY_RUN" == "true" ]]; then
        echo "Dry run — would execute:"
        echo "  Mode:       $CFG_MODE"
        echo "  Max tests:  $CFG_MAX"
        echo "  Workers:    $CFG_WORKERS"
        echo "  Timeout:    ${CFG_TIMEOUT}s"
        echo "  Categories: $CFG_CATEGORIES"
        echo "  Verbose:    $CFG_VERBOSE"
        [[ -n "$CFG_FILTER" ]] && echo "  Filter:     $CFG_FILTER"
        exit 0
    fi

    if [[ "$CFG_PASS_RATE_ONLY" == "true" ]]; then
        local temp_output
        temp_output=$(mktemp)
        trap "rm -f $temp_output" EXIT

        run_mode 2>&1 | tee "$temp_output"
        local exit_code=${PIPESTATUS[0]}

        local pass_line
        pass_line=$(grep -E "^Pass Rate:" "$temp_output" | tail -1)
        if [[ -n "$pass_line" ]]; then
            echo ""
            echo "===PASS_RATE_START==="
            echo "$pass_line" | grep -oE '[0-9]+\.[0-9]+'
            echo "===PASS_RATE_END==="
        fi

        rm -f "$temp_output"
        exit $exit_code
    fi

    run_mode
}

main "$@"
