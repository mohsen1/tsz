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
#   --max=N               Maximum tests (default: 500)
#   --filter=PATTERN      Filter tests by name
#   --concurrency=N, -jN  Parallel workers (default: CPU count)
#   --timeout=MS          Per-test timeout in ms (default: 5000)
#   --verbose             Detailed output
#   --js-only             Test JavaScript emit only
#   --dts-only            Test declaration emit only
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

# Resolve tsz binary path for the Node runner
resolve_tsz_binary() {
    local candidates=()

    if [[ -n "${TSZ_BIN:-}" ]]; then
        candidates+=("$TSZ_BIN")
    fi

    if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
        candidates+=("$CARGO_TARGET_DIR/release/tsz")
    fi

    candidates+=(
        "$ROOT_DIR/.target/release/tsz"
        "$ROOT_DIR/target/release/tsz"
    )

    for tsz_bin in "${candidates[@]}"; do
        if [[ -x "$tsz_bin" ]]; then
            TSZ_BIN="$tsz_bin"
            export TSZ_BIN
            return 0
        fi
    done

    log_error "tsz binary not found in known target directories"
    return 1
}

rebuild_tsz_binary() {
    log_step "Building tsz binary..."
    (
        cd "$ROOT_DIR"
        CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-.target}" cargo build --release -p tsz-cli --bin tsz
    )
    log_success "tsz binary built"
}

ensure_tsz_binary() {
    if ! resolve_tsz_binary; then
        rebuild_tsz_binary
        resolve_tsz_binary || {
            log_error "Failed to resolve tsz binary after build"
            exit 1
        }
        return 0
    fi

    local tsz_bin="$TSZ_BIN"
    local stale=0

    # Rebuild automatically when emitter/checker/cli sources changed after the binary.
    if find \
        "$ROOT_DIR/src" \
        "$ROOT_DIR/crates/tsz-cli/src" \
        "$ROOT_DIR/crates/tsz-emitter/src" \
        "$ROOT_DIR/crates/tsz-checker/src" \
        "$ROOT_DIR/crates/tsz-solver/src" \
        "$ROOT_DIR/Cargo.toml" \
        "$ROOT_DIR/Cargo.lock" \
        -type f -newer "$tsz_bin" 2>/dev/null | grep -q .; then
        stale=1
    fi

    if [[ "$stale" -eq 1 ]]; then
        log_info "Detected stale tsz binary; rebuilding"
        rebuild_tsz_binary
        resolve_tsz_binary || {
            log_error "Failed to resolve tsz binary after rebuild"
            exit 1
        }
    fi
}

# Build TypeScript runner
build_runner() {
    local dist_runner="$SCRIPT_DIR/dist/runner.js"
    local should_build=0

    if [[ ! -f "$dist_runner" ]]; then
        should_build=1
    else
        if find "$SCRIPT_DIR/src" -type f -name '*.ts' -newer "$dist_runner" | grep -q .; then
            should_build=1
        elif [[ -f "$SCRIPT_DIR/package.json" && "$SCRIPT_DIR/package.json" -nt "$dist_runner" ]]; then
            should_build=1
        elif [[ -f "$SCRIPT_DIR/package-lock.json" && "$SCRIPT_DIR/package-lock.json" -nt "$dist_runner" ]]; then
            should_build=1
        fi
    fi

    if [[ "$should_build" -eq 0 ]]; then
        log_success "Runner up to date"
        return 0
    fi

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

    ensure_tsz_binary
    build_runner

    log_step "Running emit tests..."
    echo ""

    cd "$SCRIPT_DIR"
    node dist/runner.js "$@"
}

main "$@"
