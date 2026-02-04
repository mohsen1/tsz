#!/usr/bin/env bash
#
# TSZ Emit Test Runner
# ====================
#
# Tests tsz JavaScript and declaration emit against TypeScript's baselines.
#
# Usage: ./run.sh [options]
#
# Options:
#   --max=N           Maximum tests (default: 500)
#   --filter=PATTERN  Filter tests by name
#   --verbose         Detailed output
#   --js-only         Test JavaScript emit only
#   --dts-only        Test declaration emit only
#
# Examples:
#   ./run.sh                     # Run with defaults
#   ./run.sh --max=100           # Run 100 tests
#   ./run.sh --filter=class      # Run class-related tests
#   ./run.sh --js-only --verbose # Verbose JS-only tests

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Colors
if [[ -t 1 ]]; then
    RED='\033[0;31m' GREEN='\033[0;32m' YELLOW='\033[0;33m'
    BLUE='\033[0;34m' CYAN='\033[0;36m' BOLD='\033[1m'
    DIM='\033[2m' RESET='\033[0m'
else
    RED='' GREEN='' YELLOW='' BLUE='' CYAN='' BOLD='' DIM='' RESET=''
fi

log_info()    { echo -e "${BLUE}ℹ${RESET}  $*"; }
log_success() { echo -e "${GREEN}✓${RESET}  $*"; }
log_error()   { echo -e "${RED}✗${RESET}  $*" >&2; }
log_step()    { echo -e "${CYAN}→${RESET}  $*"; }

die() { log_error "$@"; exit 2; }

# Check for required tools
command -v node &>/dev/null || die "Node.js is required"

# Check for tsz binary
check_tsz_binary() {
    local tsz_bin="$ROOT_DIR/target/release/tsz"
    if [[ ! -f "$tsz_bin" ]]; then
        log_error "tsz binary not found at $tsz_bin"
        log_info "Build it with: cargo build --release"
        exit 1
    fi
}

# Build TypeScript runner
build_runner() {
    log_step "Building emit runner..."
    (
        cd "$SCRIPT_DIR"
        if [[ ! -d "node_modules" ]]; then
            npm install --silent 2>/dev/null || npm install
        fi
        npm run build --silent 2>/dev/null || npm run build
    )
    log_success "Runner built"
}

# Main
main() {
    # Check baselines exist
    local baselines_dir="$ROOT_DIR/TypeScript/tests/baselines/reference"
    if [[ ! -d "$baselines_dir" ]]; then
        die "TypeScript baselines not found. Run: ./scripts/setup-ts-submodule.sh"
    fi

    check_tsz_binary
    build_runner

    log_step "Running emit tests..."
    echo ""

    cd "$SCRIPT_DIR"
    node dist/runner.js "$@"
}

main "$@"
