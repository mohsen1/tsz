#!/usr/bin/env bash
#
# TSZ Conformance Test Runner
# ===========================
#
# A unified script for running TypeScript conformance tests against the tsz compiler.
# Supports multiple execution modes: WASM in Docker, native binary in Docker, or native
# binary directly on the host.
#
# Usage: ./run.sh [command] [options]
#
# Examples:
#   ./run.sh                          # Run with defaults (WASM + Docker, 500 tests)
#   ./run.sh --all                    # Run all tests
#   ./run.sh --native --no-docker     # Run native binary without Docker (macOS)
#   ./run.sh --max=100 --verbose      # Run 100 tests with verbose output
#   ./run.sh cache generate           # Generate TSC cache for faster runs
#   ./run.sh single path/to/test.ts   # Run a single test file
#
# For full help: ./run.sh --help
#

set -euo pipefail

# ==============================================================================
# Signal Handling (Ctrl+C)
# ==============================================================================

# Track child PIDs for cleanup
CHILD_PIDS=()
DOCKER_CONTAINER_NAME=""

cleanup() {
    local exit_code=$?
    echo "" # Newline after ^C

    # Stop Docker container if running
    if [[ -n "$DOCKER_CONTAINER_NAME" ]]; then
        echo "Stopping Docker container..."
        docker stop "$DOCKER_CONTAINER_NAME" 2>/dev/null || true
        docker rm -f "$DOCKER_CONTAINER_NAME" 2>/dev/null || true
    fi

    # Kill any child processes
    for pid in "${CHILD_PIDS[@]:-}"; do
        kill -TERM "$pid" 2>/dev/null || true
    done
    # Kill any node processes we spawned
    pkill -P $$ 2>/dev/null || true
    exit $exit_code
}

# Trap signals
trap cleanup INT TERM EXIT

# ==============================================================================
# Configuration
# ==============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DOCKER_IMAGE="tsz-conformance"

# Colors (disabled if not a terminal)
if [[ -t 1 ]]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[0;33m'
    BLUE='\033[0;34m'
    CYAN='\033[0;36m'
    BOLD='\033[1m'
    DIM='\033[2m'
    RESET='\033[0m'
else
    RED='' GREEN='' YELLOW='' BLUE='' CYAN='' BOLD='' DIM='' RESET=''
fi

# ==============================================================================
# Help System
# ==============================================================================

show_help() {
    cat << 'EOF'
TSZ Conformance Test Runner
===========================

USAGE:
    ./run.sh [command] [options]
    ./run.sh [options]

COMMANDS:
    (default)       Run conformance tests with the configured options
    cache           Manage TSC result cache (see CACHE COMMANDS below)
    single <file>   Run a single test file and show detailed output
    help            Show this help message

CACHE COMMANDS:
    cache generate  Generate/update TSC cache (speeds up subsequent runs)
    cache status    Show cache status and statistics
    cache clear     Clear the TSC cache

OPTIONS:
    Execution Mode:
    --server            Use tsz-server (persistent process, default - fastest)
    --wasm              Use WASM module (slower, but good isolation)
    --native            Use native binary spawned per test (legacy mode)
    --docker            Run inside Docker container (provides isolation)
    --no-docker         Run directly on host (default with --server)

    Test Selection:
    --all               Run all available tests (overrides --max)
    --max=N             Maximum number of tests to run (default: 500)
    --category=CAT      Test categories to run, comma-separated
                        Options: conformance, compiler, projects
                        Default: conformance,compiler,projects
    --filter=PATTERN    Only run tests matching pattern (substring match)

    Execution:
    --workers=N         Number of parallel workers (default: auto-detect)
    --timeout=SECS      Test timeout in seconds (default: 600 for normal, 3600 for --all)

    Output:
    -v, --verbose       Show detailed output for each test
    --print-test        Show detailed info for filtered tests (use with --filter)
                        Displays: file content, directives, TSC expected, tsz actual
    -q, --quiet         Minimal output (only summary)
    --json              Output results as JSON (implies --quiet)

    Other:
    -h, --help          Show this help message
    --version           Show version information
    --dry-run           Show what would be run without executing

EXAMPLES:
    # Quick test run with defaults (WASM + Docker, 500 tests)
    ./run.sh

    # Run all tests with native binary (macOS - no Docker)
    ./run.sh --native --no-docker --all --workers=8

    # Run compiler tests only, verbose output
    ./run.sh --category=compiler --verbose

    # Run specific number of tests with WASM
    ./run.sh --wasm --max=1000

    # Generate cache for faster subsequent runs
    ./run.sh cache generate

    # Test a single file
    ./run.sh single TypeScript/tests/cases/conformance/types/tuple/test.ts

EXECUTION MODES:
    The runner supports multiple execution modes:

    1. Server Mode (default, fastest)
       - Builds tsz-server binary
       - Keeps libs cached in memory
       - 5-10x faster than spawn-per-test
       - Uses stdin/stdout JSON protocol

    2. WASM + Docker (safest)
       - Builds WASM module with wasm-pack
       - Runs tests inside Docker container
       - Best isolation from infinite loops/OOM
       - Cross-platform compatible

    3. WASM + No Docker
       - Builds WASM module
       - Runs directly on host
       - Good isolation (WASM sandbox)

    4. Native + No Docker (legacy)
       - Spawns native binary per test
       - Slower due to process overhead
       - ⚠️  No isolation - infinite loops can hang your system

CONFIGURATION:
    The script respects .cargo/config.toml settings:
    - target-dir: Uses .target/ instead of target/ if configured

    Environment variables:
    - TSZ_BINARY: Override path to native binary
    - RUST_LOG: Set logging level for native binary (debug, trace, etc.)

FILES:
    conformance/
    ├── run.sh              This script
    ├── src/                TypeScript runner source
    ├── dist/               Compiled runner
    ├── .tsc-cache/         TSC result cache
    └── package.json        Node.js dependencies

EXIT CODES:
    0    All tests passed
    1    Some tests failed or crashed
    2    Invalid arguments or configuration error
    124  Timeout exceeded

EOF
}

show_version() {
    echo "TSZ Conformance Test Runner v1.0.0"
    echo "Rust: $(rustc --version 2>/dev/null || echo 'not installed')"
    echo "Node: $(node --version 2>/dev/null || echo 'not installed')"
    echo "Docker: $(docker --version 2>/dev/null || echo 'not installed')"
}

# ==============================================================================
# Utility Functions
# ==============================================================================

log_info()    { echo -e "${BLUE}ℹ${RESET}  $*"; }
log_success() { echo -e "${GREEN}✓${RESET}  $*"; }
log_warning() { echo -e "${YELLOW}⚠${RESET}  $*"; }
log_error()   { echo -e "${RED}✗${RESET}  $*" >&2; }
log_step()    { echo -e "${CYAN}→${RESET}  $*"; }

die() {
    log_error "$@"
    exit 2
}

# Detect number of CPU cores
detect_cores() {
    if [[ "$OSTYPE" == "darwin"* ]]; then
        sysctl -n hw.ncpu
    elif [[ -f /proc/cpuinfo ]]; then
        grep -c ^processor /proc/cpuinfo
    else
        echo 4
    fi
}

# Check if a command exists
require_cmd() {
    if ! command -v "$1" &>/dev/null; then
        die "Required command not found: $1"
    fi
}

# Get target directory from cargo config
get_target_dir() {
    local config_file="$ROOT_DIR/.cargo/config.toml"
    if [[ -f "$config_file" ]]; then
        local dir
        # Parse: target-dir = ".target"
        dir=$(grep -E '^target-dir' "$config_file" 2>/dev/null | awk -F'"' '{print $2}')
        if [[ -n "$dir" ]]; then
            echo "$ROOT_DIR/$dir"
            return
        fi
    fi
    echo "$ROOT_DIR/target"
}

# ==============================================================================
# Build Functions
# ==============================================================================

build_wasm() {
    log_step "Building WASM module..."
    cd "$ROOT_DIR"
    
    if ! command -v wasm-pack &>/dev/null; then
        die "wasm-pack not found. Install with: cargo install wasm-pack"
    fi
    
    wasm-pack build --target nodejs --out-dir pkg --release
    log_success "WASM module built"
    # Lib files are embedded in the binary (no copy needed).
}

build_native() {
    log_step "Building native binary..."
    cd "$ROOT_DIR"

    require_cmd cargo
    cargo build --release --bin tsz
    log_success "Native binary built"
    # Lib files are embedded in the binary (no copy needed).
}

build_server() {
    log_step "Building tsz-server..."
    cd "$ROOT_DIR"

    require_cmd cargo
    cargo build --release --bin tsz-server
    log_success "tsz-server built"
}

build_native_for_docker() {
    log_step "Building native binary for Linux (Docker)..."

    # Build the binary inside Docker to avoid cross-compilation issues
    # Use the host architecture for better performance on Apple Silicon
    log_info "Building in Docker container..."

    docker run --rm \
        -v "$ROOT_DIR:/work:rw" \
        -w /work \
        --platform linux/arm64 \
        rust:1-slim \
        bash -c "
            apt-get update -qq && apt-get install -y -qq pkg-config libssl-dev >/dev/null 2>&1
            cargo build --release --bin tsz
        "

    log_success "Native binary for Linux built"
}

build_runner() {
    log_step "Building conformance runner..."
    cd "$SCRIPT_DIR"
    
    if [[ ! -d "node_modules" ]] || [[ ! -d "node_modules/typescript" ]]; then
        npm install --silent 2>/dev/null || npm install
    fi
    
    npm run build --silent 2>/dev/null || npm run build
    log_success "Runner built"
}

# ==============================================================================
# Docker Functions
# ==============================================================================

check_docker() {
    if ! command -v docker &>/dev/null; then
        die "Docker not found. Install from: https://docs.docker.com/get-docker/"
    fi
    
    if ! docker info &>/dev/null; then
        die "Docker daemon not running. Start Docker Desktop or run: sudo systemctl start docker"
    fi
}

ensure_docker_image() {
    if ! docker image inspect "$DOCKER_IMAGE" &>/dev/null; then
        log_step "Building Docker image..."
        docker build --platform linux/arm64 -t "$DOCKER_IMAGE" -f - "$SCRIPT_DIR" << 'DOCKERFILE'
FROM node:22-slim
WORKDIR /app
RUN mkdir -p /app/conformance /app/pkg /app/target /app/TypeScript/tests
DOCKERFILE
    fi
    log_success "Docker image ready"
}

# ==============================================================================
# Cache Commands
# ==============================================================================

cmd_cache() {
    local subcmd="${1:-status}"
    
    build_runner
    
    case "$subcmd" in
        generate)
            log_step "Generating TSC cache..."
            cd "$SCRIPT_DIR"
            node dist/generate-cache.js
            ;;
        status)
            cd "$SCRIPT_DIR"
            node dist/generate-cache.js --status
            ;;
        clear)
            log_step "Clearing TSC cache..."
            cd "$SCRIPT_DIR"
            node dist/generate-cache.js --clear
            log_success "Cache cleared"
            ;;
        *)
            die "Unknown cache command: $subcmd (use: generate, status, clear)"
            ;;
    esac
}

# ==============================================================================
# Single Test Command
# ==============================================================================

cmd_single() {
    local test_file="$1"
    
    if [[ -z "$test_file" ]]; then
        die "Usage: ./run.sh single <path/to/test.ts>"
    fi
    
    if [[ ! -f "$test_file" ]] && [[ ! -f "$ROOT_DIR/$test_file" ]]; then
        die "Test file not found: $test_file"
    fi
    
    # Resolve to absolute path
    if [[ ! "$test_file" = /* ]]; then
        if [[ -f "$ROOT_DIR/$test_file" ]]; then
            test_file="$ROOT_DIR/$test_file"
        else
            test_file="$(pwd)/$test_file"
        fi
    fi
    
    build_native
    
    local target_dir
    target_dir=$(get_target_dir)
    local binary="$target_dir/release/tsz"
    
    echo ""
    echo -e "${BOLD}Running: ${RESET}$test_file"
    echo "─────────────────────────────────────────────────────────────"
    echo ""
    
    cd "$ROOT_DIR"
    "$binary" "$test_file" 2>&1 || true
    
    echo ""
    echo "─────────────────────────────────────────────────────────────"
}

# ==============================================================================
# Main Test Runner
# ==============================================================================

run_tests() {
    local use_wasm="$1"
    local use_docker="$2"
    local max_tests="$3"
    local workers="$4"
    local timeout="$5"
    local categories="$6"
    local verbose="$7"
    
    # Validate mode combinations
    if [[ "$use_docker" == "true" ]] && [[ "$use_wasm" == "false" ]] && [[ "$OSTYPE" == "darwin"* ]]; then
        echo ""
        log_warning "Native binary + Docker may not work on macOS!"
        echo ""
        echo "  The macOS-compiled binary cannot run inside the Linux Docker container."
        echo "  To use native mode with Docker, you need to build for Linux target."
        echo ""
        echo "  Options:"
        echo "    1. Use --no-docker to run native binary directly (recommended)"
        echo "    2. Use --wasm with Docker (platform-independent)"
        echo "    3. Build for Linux: cargo build --release --target x86_64-unknown-linux-gnu"
        echo ""
        # Commented out to allow users to try it if they want
        # exit 2
    fi
    
    # Print banner
    local mode_desc
    if [[ "$use_wasm" == "true" ]]; then
        mode_desc="WASM"
    else
        mode_desc="Native"
    fi
    if [[ "$use_docker" == "true" ]]; then
        mode_desc="$mode_desc + Docker"
    else
        mode_desc="$mode_desc (direct)"
    fi
    
    echo ""
    echo -e "${CYAN}╔══════════════════════════════════════════════════════════════╗${RESET}"
    echo -e "${CYAN}║${RESET}${BOLD}         TSZ Conformance Test Runner                          ${RESET}${CYAN}║${RESET}"
    echo -e "${CYAN}╠══════════════════════════════════════════════════════════════╣${RESET}"
    echo -e "${CYAN}║${RESET}  Mode:       $(printf '%-48s' "$mode_desc")${CYAN}║${RESET}"
    echo -e "${CYAN}║${RESET}  Tests:      $(printf '%-48s' "$max_tests")${CYAN}║${RESET}"
    echo -e "${CYAN}║${RESET}  Workers:    $(printf '%-48s' "$workers")${CYAN}║${RESET}"
    echo -e "${CYAN}║${RESET}  Categories: $(printf '%-48s' "$categories")${CYAN}║${RESET}"
    echo -e "${CYAN}║${RESET}  Timeout:    $(printf '%-48s' "${timeout}s")${CYAN}║${RESET}"
    echo -e "${CYAN}╚══════════════════════════════════════════════════════════════╝${RESET}"
    echo ""
    
    # Build phase
    if [[ "$use_wasm" == "true" ]]; then
        build_wasm
    else
        if [[ "$use_docker" == "true" ]] && [[ "$OSTYPE" == "darwin"* ]]; then
            build_native_for_docker
        else
            build_native
        fi
    fi
    
    build_runner
    
    # Build runner args
    local runner_args="--max=$max_tests --workers=$workers --category=$categories --wasm=$use_wasm"
    if [[ "$verbose" == "true" ]]; then
        runner_args="$runner_args --verbose"
    fi
    
    if [[ "$use_docker" == "true" ]]; then
        run_in_docker "$use_wasm" "$workers" "$timeout" "$runner_args"
    else
        run_direct "$use_wasm" "$timeout" "$runner_args"
    fi
}

run_in_docker() {
    local use_wasm="$1"
    local workers="$2"
    local timeout="$3"
    local runner_args="$4"
    
    check_docker
    ensure_docker_image
    
    # Calculate memory: ~3GB per worker for WASM (needs more headroom), minimum 8GB
    local memory_gb=$(( workers * 3 ))
    if (( memory_gb < 8 )); then memory_gb=8; fi
    
    log_step "Running tests in Docker (Memory: ${memory_gb}GB, CPUs: $workers)..."
    echo ""
    
    local mount_dir
    if [[ "$use_wasm" == "true" ]]; then
        mount_dir="$ROOT_DIR/pkg"
    else
        mount_dir="$(get_target_dir)"
    fi
    
    # Generate unique container name for cleanup
    DOCKER_CONTAINER_NAME="tsz-conformance-$$"

    docker run --rm \
        --name "$DOCKER_CONTAINER_NAME" \
        --platform linux/arm64 \
        --memory="${memory_gb}g" \
        --memory-swap="${memory_gb}g" \
        --cpus="$workers" \
        --pids-limit=1000 \
        -v "$mount_dir:/app/target:ro" \
        -v "$ROOT_DIR/pkg:/app/pkg:ro" \
        -v "$SCRIPT_DIR/src:/app/conformance/src:ro" \
        -v "$SCRIPT_DIR/dist:/app/conformance/dist:ro" \
        -v "$SCRIPT_DIR/package.json:/app/conformance/package.json:ro" \
        -v "$SCRIPT_DIR/.tsc-cache:/app/conformance/.tsc-cache:ro" \
        -v "$ROOT_DIR/TypeScript/tests:/app/TypeScript/tests:ro" \
        -v "$ROOT_DIR/TypeScript/src/lib:/app/TypeScript/src/lib:ro" \
        "$DOCKER_IMAGE" sh -c "
            cd /app/conformance
            npm install --silent 2>/dev/null || true
            timeout ${timeout}s node --expose-gc dist/runner.js $runner_args
            EXIT_CODE=\$?
            if [ \$EXIT_CODE -eq 124 ]; then
                echo ''
                echo '⏱️  Tests timed out after ${timeout}s'
            fi
            exit \$EXIT_CODE
        "
    local docker_exit=$?
    DOCKER_CONTAINER_NAME=""  # Clear so cleanup doesn't try to stop it
    return $docker_exit
}

run_direct() {
    local use_wasm="$1"
    local timeout="$2"
    local runner_args="$3"
    
    if [[ "$use_wasm" == "false" ]]; then
        log_warning "Running without Docker isolation. Infinite loops may hang your system."
        echo ""
    fi
    
    log_step "Running tests..."
    echo ""
    
    cd "$SCRIPT_DIR"
    
    # Set native binary path if using native mode
    if [[ "$use_wasm" == "false" ]]; then
        local target_dir
        if [[ "$use_docker" == "true" ]] && [[ "$OSTYPE" == "darwin"* ]]; then
            target_dir="$ROOT_DIR/.target/linux"
        else
            target_dir=$(get_target_dir)
        fi
        export TSZ_BINARY="$target_dir/tsz"
    fi
    
    # Run with timeout
    if command -v timeout &>/dev/null; then
        timeout "${timeout}s" node --expose-gc dist/runner.js $runner_args || {
            local exit_code=$?
            if [[ $exit_code -eq 124 ]]; then
                echo ""
                log_warning "Tests timed out after ${timeout}s"
            fi
            return $exit_code
        }
    else
        # macOS doesn't have timeout by default
        node --expose-gc dist/runner.js $runner_args
    fi
}

run_server() {
    local max_tests="$1"
    local workers="$2"
    local timeout="$3"
    local categories="$4"
    local verbose="$5"
    local filter="${6:-}"
    local print_test="${7:-false}"

    # Print banner (skip for print-test mode to keep output clean)
    if [[ "$print_test" != "true" ]]; then
        echo ""
        echo -e "${CYAN}╔══════════════════════════════════════════════════════════════╗${RESET}"
        echo -e "${CYAN}║${RESET}${BOLD}         TSZ Conformance Test Runner                          ${RESET}${CYAN}║${RESET}"
        echo -e "${CYAN}╠══════════════════════════════════════════════════════════════╣${RESET}"
        echo -e "${CYAN}║${RESET}  Mode:       $(printf '%-48s' "Server (persistent)")${CYAN}║${RESET}"
        echo -e "${CYAN}║${RESET}  Tests:      $(printf '%-48s' "$max_tests")${CYAN}║${RESET}"
        echo -e "${CYAN}║${RESET}  Workers:    $(printf '%-48s' "$workers")${CYAN}║${RESET}"
        echo -e "${CYAN}║${RESET}  Categories: $(printf '%-48s' "$categories")${CYAN}║${RESET}"
        echo -e "${CYAN}║${RESET}  Timeout:    $(printf '%-48s' "${timeout}s")${CYAN}║${RESET}"
        if [[ -n "$filter" ]]; then
            echo -e "${CYAN}║${RESET}  Filter:     $(printf '%-48s' "$filter")${CYAN}║${RESET}"
        fi
        echo -e "${CYAN}╚══════════════════════════════════════════════════════════════╝${RESET}"
        echo ""
    fi

    build_server
    build_runner

    if [[ "$print_test" != "true" ]]; then
        log_step "Starting tsz-server pool..."
        echo ""
    fi

    cd "$SCRIPT_DIR"

    # Set server binary path
    local target_dir
    target_dir=$(get_target_dir)
    export TSZ_SERVER_BINARY="$target_dir/release/tsz-server"
    export TSZ_LIB_DIR="$ROOT_DIR/TypeScript/src/lib"

    # Build runner args
    local runner_args="--max=$max_tests --workers=$workers --category=$categories --server"
    if [[ "$verbose" == "true" ]]; then
        runner_args="$runner_args --verbose"
    fi
    if [[ -n "$filter" ]]; then
        runner_args="$runner_args --filter=$filter"
    fi
    if [[ "$print_test" == "true" ]]; then
        runner_args="$runner_args --print-test"
    fi

    # Run with timeout
    if command -v timeout &>/dev/null; then
        timeout "${timeout}s" node --expose-gc dist/runner.js $runner_args || {
            local exit_code=$?
            if [[ $exit_code -eq 124 ]]; then
                echo ""
                log_warning "Tests timed out after ${timeout}s"
            fi
            return $exit_code
        }
    else
        # macOS doesn't have timeout by default
        node --expose-gc dist/runner.js $runner_args
    fi
}

# ==============================================================================
# Argument Parsing
# ==============================================================================

main() {
    # Defaults - server mode is now the default (fastest)
    local mode="server"  # server, wasm, native
    local use_docker=false
    local max_tests=500
    local workers=""
    local timeout=600
    local categories="conformance,compiler"
    local verbose=false
    local dry_run=false
    local command=""
    local positional_args=()
    local filter=""
    local print_test=false
    
    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case "$1" in
            # Commands
            help|--help|-h)
                show_help
                exit 0
                ;;
            --version)
                show_version
                exit 0
                ;;
            cache)
                command="cache"
                shift
                positional_args+=("$@")
                break
                ;;
            single)
                command="single"
                shift
                positional_args+=("$@")
                break
                ;;
            
            # Mode options
            --server)
                mode="server"
                use_docker=false  # Server mode doesn't use Docker
                ;;
            --wasm)
                mode="wasm"
                ;;
            --native)
                mode="native"
                ;;
            --docker)
                use_docker=true
                ;;
            --no-docker|--no-sandbox)
                use_docker=false
                ;;
            
            # Test selection
            --all)
                max_tests=99999
                timeout=3600
                ;;
            --max=*)
                max_tests="${1#*=}"
                ;;
            --category=*)
                categories="${1#*=}"
                ;;
            --filter=*)
                filter="${1#*=}"
                ;;
            --print-test)
                print_test=true
                ;;
            
            # Execution
            --workers=*)
                workers="${1#*=}"
                ;;
            --timeout=*)
                timeout="${1#*=}"
                ;;
            
            # Output
            -v|--verbose)
                verbose=true
                ;;
            -q|--quiet)
                # TODO: Implement quiet mode
                ;;
            --json)
                # TODO: Implement JSON output
                log_warning "--json not yet implemented"
                ;;
            
            # Other
            --dry-run)
                dry_run=true
                ;;
            
            # Unknown
            -*)
                die "Unknown option: $1 (use --help for usage)"
                ;;
            *)
                positional_args+=("$1")
                ;;
        esac
        shift
    done
    
    # Auto-detect workers if not specified
    if [[ -z "$workers" ]]; then
        workers=$(detect_cores)
        # Cap at reasonable number for default runs
        if (( workers > 8 )) && (( max_tests < 1000 )); then
            workers=8
        fi
        # Cap WASM + Docker at 8 workers to prevent OOM (WASM needs ~3GB per worker)
        if [[ "$mode" == "wasm" ]] && [[ "$use_docker" == "true" ]] && (( workers > 8 )); then
            log_warning "Capping WASM+Docker workers to 8 (was $workers) to prevent OOM"
            workers=8
        fi
    fi
    
    # Handle commands
    case "$command" in
        cache)
            cmd_cache "${positional_args[@]:-status}"
            exit $?
            ;;
        single)
            cmd_single "${positional_args[@]:-}"
            exit $?
            ;;
    esac
    
    # Dry run
    if [[ "$dry_run" == "true" ]]; then
        echo "Dry run - would execute:"
        echo "  Mode: $mode"
        echo "  Docker: $use_docker"
        echo "  Max tests: $max_tests"
        echo "  Workers: $workers"
        echo "  Timeout: ${timeout}s"
        echo "  Categories: $categories"
        echo "  Verbose: $verbose"
        exit 0
    fi

    # Run tests based on mode
    case "$mode" in
        server)
            run_server "$max_tests" "$workers" "$timeout" "$categories" "$verbose" "$filter" "$print_test"
            ;;
        wasm)
            local use_wasm=true
            run_tests "$use_wasm" "$use_docker" "$max_tests" "$workers" "$timeout" "$categories" "$verbose"
            ;;
        native)
            local use_wasm=false
            run_tests "$use_wasm" "$use_docker" "$max_tests" "$workers" "$timeout" "$categories" "$verbose"
            ;;
    esac
}

# ==============================================================================
# Entry Point
# ==============================================================================

main "$@"
