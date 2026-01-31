#!/usr/bin/env bash
#
# TSZ Fourslash Test Runner
# =========================
#
# Fully automatic script to run TypeScript's fourslash language service tests
# against tsz-server. Handles all build steps: TypeScript harness, tsz-server,
# and test execution.
#
# Usage:
#   ./scripts/run-fourslash.sh [options]
#
# Examples:
#   ./scripts/run-fourslash.sh                    # Run all fourslash tests
#   ./scripts/run-fourslash.sh --max=10           # Run first 10 tests
#   ./scripts/run-fourslash.sh --filter=quickInfo  # Run tests matching pattern
#   ./scripts/run-fourslash.sh --verbose          # Show each test result
#   ./scripts/run-fourslash.sh --server-tests     # Run server-specific tests
#   ./scripts/run-fourslash.sh --skip-build       # Skip build steps
#
# For full help: ./scripts/run-fourslash.sh --help
#

set -euo pipefail

# ==============================================================================
# Signal Handling
# ==============================================================================

CHILD_PIDS=()

cleanup() {
    local exit_code=$?
    for pid in "${CHILD_PIDS[@]:-}"; do
        kill -TERM "$pid" 2>/dev/null || true
    done
    pkill -P $$ 2>/dev/null || true
    exit $exit_code
}

trap cleanup INT TERM EXIT

# ==============================================================================
# Configuration
# ==============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
TS_DIR="$ROOT_DIR/TypeScript"
FOURSLASH_DIR="$ROOT_DIR/fourslash"

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
# Help
# ==============================================================================

show_help() {
    cat << 'EOF'
TSZ Fourslash Test Runner
=========================

Runs TypeScript's fourslash language service tests against tsz-server.

USAGE:
    ./scripts/run-fourslash.sh [options]

OPTIONS:
    Test Selection:
    --max=N             Maximum number of tests to run (default: all)
    --filter=PATTERN    Only run tests matching pattern (substring match)
    --server-tests      Run server-specific tests (tests/cases/fourslash/server/)

    Parallelism:
    --workers=N         Number of parallel workers (default: CPU count)
    --sequential        Run tests sequentially (single process, no workers)
    --timeout=MS        Per-test timeout in ms (default: 15000)
    --memory-limit=MB   Per-worker memory limit in MB (default: 512)

    Build:
    --skip-build        Skip all build steps (use existing binaries)
    --skip-ts-build     Skip TypeScript build (use existing harness)
    --skip-cargo-build  Skip cargo build (use existing tsz-server)

    Output:
    --verbose           Show detailed output for each test

    Other:
    -h, --help          Show this help message

EXAMPLES:
    # Run all tests (parallel by default)
    ./scripts/run-fourslash.sh

    # Quick smoke test with 10 tests
    ./scripts/run-fourslash.sh --max=10

    # Run with 4 parallel workers
    ./scripts/run-fourslash.sh --workers=4

    # Run quickInfo-related tests with verbose output
    ./scripts/run-fourslash.sh --filter=quickInfo --verbose

    # Run with existing builds (faster iteration)
    ./scripts/run-fourslash.sh --skip-build --max=50

ARCHITECTURE:
    The runner works by:
    1. Building TypeScript's test harness (non-bundled CJS modules)
    2. Building tsz-server (Rust binary)
    3. Forking N child processes, each with its own tsz-server instance
    4. Each child loads harness, monkey-patches TestState to route to tsz-server
    5. Tests distributed round-robin across workers
    6. Results collected via IPC and aggregated

EOF
}

# ==============================================================================
# Utilities
# ==============================================================================

log_info()    { echo -e "${BLUE}i${RESET}  $*"; }
log_success() { echo -e "${GREEN}+${RESET}  $*"; }
log_warning() { echo -e "${YELLOW}!${RESET}  $*"; }
log_error()   { echo -e "${RED}x${RESET}  $*" >&2; }
log_step()    { echo -e "${CYAN}>${RESET}  $*"; }

die() {
    log_error "$@"
    exit 2
}

require_cmd() {
    if ! command -v "$1" &>/dev/null; then
        die "Required command not found: $1"
    fi
}

# Get cargo target directory
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

# ==============================================================================
# Build Functions
# ==============================================================================

ensure_submodule() {
    if [[ ! -d "$TS_DIR/src" ]]; then
        log_step "Initializing TypeScript submodule..."
        cd "$ROOT_DIR"
        git submodule update --init --depth 1 TypeScript
        log_success "Submodule initialized"
    fi
}

build_typescript_harness() {
    log_step "Building TypeScript harness (non-bundled)..."
    cd "$TS_DIR"

    # Install dependencies if needed
    if [[ ! -d "node_modules" ]] || [[ ! -d "node_modules/typescript" ]]; then
        log_info "Installing TypeScript dependencies..."
        npm ci --silent 2>/dev/null || npm ci
    fi

    # Check if harness is already built
    if [[ -f "built/local/harness/fourslashImpl.js" ]]; then
        log_success "TypeScript harness already built"
        return
    fi

    # Generate diagnostics (needed before compiling)
    log_info "Generating diagnostics..."
    node scripts/processDiagnosticMessages.mjs src/compiler/diagnosticMessages.json

    # Generate lib files
    log_info "Generating lib files..."
    npx hereby lib 2>/dev/null || {
        # Fallback: generate libs manually
        log_warning "hereby lib failed, attempting manual lib generation..."
        node -e "
            const fs = require('fs');
            const path = require('path');
            const libs = JSON.parse(fs.readFileSync('src/lib/libs.json', 'utf-8'));
            fs.mkdirSync('built/local', { recursive: true });
            const copyright = fs.readFileSync('scripts/CopyrightNotice.txt', 'utf-8');
            for (const lib of libs.libs) {
                const sources = [lib + '.d.ts'];
                const target = libs.paths && libs.paths[lib] || ('lib.' + lib + '.d.ts');
                let output = copyright;
                for (const source of sources) {
                    output += '\n' + fs.readFileSync(path.join('src/lib', source), 'utf-8').replace(/\r\n/g, '\n');
                }
                fs.writeFileSync(path.join('built/local', target), output);
            }
        "
    }

    # Build with tsc (non-bundled: emit actual JS files)
    log_info "Compiling test harness with tsc..."
    node node_modules/typescript/lib/tsc.js -b src/testRunner --emitDeclarationOnly false

    if [[ -f "built/local/harness/fourslashImpl.js" ]]; then
        log_success "TypeScript harness built"
    else
        die "TypeScript harness build failed: built/local/harness/fourslashImpl.js not found"
    fi
}

build_tsz_server() {
    log_step "Building tsz-server..."
    cd "$ROOT_DIR"

    require_cmd cargo
    cargo build --release --bin tsz-server 2>&1

    local target_dir
    target_dir=$(get_target_dir)
    if [[ -f "$target_dir/release/tsz-server" ]]; then
        log_success "tsz-server built: $target_dir/release/tsz-server"
    else
        die "tsz-server build failed"
    fi
}

# ==============================================================================
# Main
# ==============================================================================

main() {
    # Parse arguments
    local skip_build=false
    local skip_ts_build=false
    local skip_cargo_build=false
    local runner_args=()

    for arg in "$@"; do
        case "$arg" in
            -h|--help)
                show_help
                exit 0
                ;;
            --skip-build)
                skip_build=true
                ;;
            --skip-ts-build)
                skip_ts_build=true
                ;;
            --skip-cargo-build)
                skip_cargo_build=true
                ;;
            *)
                runner_args+=("$arg")
                ;;
        esac
    done

    echo ""
    echo -e "${CYAN}=== TSZ Fourslash Test Runner ===${RESET}"
    echo ""

    # Check prerequisites
    require_cmd node
    require_cmd npm

    # Ensure submodule is initialized
    ensure_submodule

    # Build steps
    if [[ "$skip_build" == "false" ]]; then
        if [[ "$skip_ts_build" == "false" ]]; then
            build_typescript_harness
        else
            log_info "Skipping TypeScript build"
        fi

        if [[ "$skip_cargo_build" == "false" ]]; then
            build_tsz_server
        else
            log_info "Skipping cargo build"
        fi
    else
        log_info "Skipping all build steps"
    fi

    # Resolve tsz-server path
    local target_dir
    target_dir=$(get_target_dir)
    local tsz_server_binary="$target_dir/release/tsz-server"

    if [[ ! -f "$tsz_server_binary" ]]; then
        die "tsz-server binary not found at: $tsz_server_binary"
    fi

    # Run the fourslash tests
    echo ""
    log_step "Running fourslash tests..."
    echo ""

    cd "$TS_DIR"
    node "$FOURSLASH_DIR/runner.js" \
        --tsz-server="$tsz_server_binary" \
        "${runner_args[@]+"${runner_args[@]}"}"
}

main "$@"
