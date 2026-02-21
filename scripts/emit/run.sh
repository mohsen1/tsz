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
#   --skip-build, --no-build
#                         Skip tsz/baseline rebuild checks (requires prebuilt artifacts)
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

# Files that can affect tsz semantic output.
TSZ_WATCH_PATHS=(
    "$ROOT_DIR/src"
    "$ROOT_DIR/crates/tsz-cli/src"
    "$ROOT_DIR/crates/tsz-emitter/src"
    "$ROOT_DIR/crates/tsz-checker/src"
    "$ROOT_DIR/crates/tsz-solver/src"
    "$ROOT_DIR/crates/tsz-parser/src"
    "$ROOT_DIR/crates/tsz-scanner/src"
    "$ROOT_DIR/crates/tsz-common/src"
    "$ROOT_DIR/Cargo.toml"
    "$ROOT_DIR/Cargo.lock"
)
RUNNER_WATCH_PATHS=(
    "$SCRIPT_DIR/src"
    "$SCRIPT_DIR/tsconfig.json"
    "$SCRIPT_DIR/../package.json"
    "$SCRIPT_DIR/../package-lock.json"
)

# Check for required tools
command -v node &>/dev/null || die "Node.js is required"

print_help() {
    cat <<'EOF'
Usage: ./run.sh [options]

Options:
  --max=N               Maximum tests (default: all)
  --filter=PATTERN      Filter tests by name
  --concurrency=N, -jN  Parallel workers (default: CPU count)
  --timeout=MS          Per-test timeout in ms (default: 5000)
  --skip-build, --no-build
                        Skip rebuild checks for tsz and runner (requires prebuilt artifacts)
  --verbose, -v         Detailed output with diffs
  --js-only             Test JavaScript emit only
  --dts-only            Test declaration emit only
  --help, -h            Show this help
EOF
}

resolve_tsc_binary() {
    local scripts_dir
    scripts_dir="$(cd "$SCRIPT_DIR/.." && pwd)"

    local candidates=(
        "$scripts_dir/node_modules/.bin/tsc"
        "$SCRIPT_DIR/node_modules/.bin/tsc"
    )

    for candidate in "${candidates[@]}"; do
        if [[ -x "$candidate" ]]; then
            TSC_BIN="$candidate"
            export TSC_BIN
            return 0
        fi
    done

    return 1
}

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
    local current_head
    local state_file="$(dirname "$tsz_bin")/.tsz_binary_head"

    if command -v git &>/dev/null && git -C "$ROOT_DIR" rev-parse --is-inside-work-tree &>/dev/null; then
        if [[ -n "$(git -C "$ROOT_DIR" status --porcelain -- "${TSZ_WATCH_PATHS[@]}" 2>/dev/null)" ]]; then
            stale=1
        else
            current_head="$(git -C "$ROOT_DIR" rev-parse HEAD 2>/dev/null || true)"
            if [[ -z "$current_head" ]]; then
                stale=1
            elif [[ ! -f "$state_file" ]]; then
                stale=1
            elif [[ "$current_head" != "$(cat "$state_file")" ]]; then
                stale=1
            fi
        fi
    else
        # Fallback when not in a git checkout: use filesystem mtime checks.
        if find "${TSZ_WATCH_PATHS[@]}" -type f -newer "$tsz_bin" 2>/dev/null | grep -q .; then
            stale=1
        fi
    fi

    if [[ "$stale" -eq 0 ]]; then
        current_head="$(git -C "$ROOT_DIR" rev-parse HEAD 2>/dev/null || true)"
        if [[ -n "$current_head" ]]; then
            mkdir -p "$(dirname "$state_file")"
            printf '%s\n' "$current_head" > "$state_file"
        fi
        return 0
    fi

    if [[ "$stale" -eq 1 ]]; then
        log_info "Detected stale tsz binary; rebuilding"
        rebuild_tsz_binary
        resolve_tsz_binary || {
            log_error "Failed to resolve tsz binary after rebuild"
            exit 1
        }
        current_head="$(git -C "$ROOT_DIR" rev-parse HEAD 2>/dev/null || true)"
        if [[ -n "$current_head" ]]; then
            mkdir -p "$(dirname "$state_file")"
            printf '%s\n' "$current_head" > "$state_file"
        fi
    fi
}

# Build TypeScript runner
build_runner() {
    local dist_runner="$SCRIPT_DIR/dist/runner.js"
    local stale=0
    local current_head
    local state_file="$(dirname "$dist_runner")/.runner_build_head"

    if [[ ! -f "$dist_runner" ]]; then
        stale=1
    else
    if command -v git &>/dev/null && git -C "$ROOT_DIR" rev-parse --is-inside-work-tree &>/dev/null; then
            if [[ -n "$(git -C "$ROOT_DIR" status --porcelain -- "${RUNNER_WATCH_PATHS[@]}" 2>/dev/null)" ]]; then
                stale=1
            else
                current_head="$(git -C "$ROOT_DIR" rev-parse HEAD 2>/dev/null || true)"
                if [[ -z "$current_head" || ! -f "$state_file" || "$current_head" != "$(cat "$state_file")" ]]; then
                    stale=1
                fi
            fi
        else
            # Fallback when not in a git checkout: use filesystem mtime checks.
            if find "${RUNNER_WATCH_PATHS[@]}" -type f -newer "$dist_runner" 2>/dev/null | grep -q .; then
                stale=1
            fi
        fi

        if [[ "$stale" -eq 0 ]]; then
            current_head="$(git -C "$ROOT_DIR" rev-parse HEAD 2>/dev/null || true)"
            if [[ -n "$current_head" ]]; then
                mkdir -p "$(dirname "$state_file")"
                printf '%s\n' "$current_head" > "$state_file"
            fi
            log_success "Runner up to date"
            return 0
        fi
    fi

    log_step "Building emit runner..."
    local scripts_dir
    scripts_dir="$(cd "$SCRIPT_DIR/.." && pwd)"
    # Install from the consolidated scripts/ package (parent of emit/)
    if [[ ! -d "$scripts_dir/node_modules" ]]; then
        log_step "Installing scripts dependencies..."
        (cd "$scripts_dir" && npm install --silent 2>/dev/null || npm install)
    fi

    if [[ ! -x "$scripts_dir/node_modules/.bin/tsc" ]]; then
        log_step "Installing TypeScript in scripts dependencies..."
        (cd "$scripts_dir" && npm install typescript --include=dev --no-fund --no-audit)
    fi

    # Re-check legacy location for older layouts where dependencies may live
    # under `scripts/emit/node_modules`.
    if [[ ! -x "$scripts_dir/node_modules/.bin/tsc" && -d "$SCRIPT_DIR/node_modules" ]]; then
        log_info "TS compiler not available in scripts/node_modules; using scripts/emit/node_modules fallback"
    fi

    if ! resolve_tsc_binary; then
        if [[ -f "$scripts_dir/package.json" || -f "$scripts_dir/package-lock.json" ]]; then
            log_step "Trying scripts package dependencies fallback..."
            if [[ ! -d "$scripts_dir/node_modules" ]]; then
                log_step "Installing scripts package dependencies..."
                (cd "$scripts_dir" && npm install --include=dev --no-fund --no-audit)
            fi
            resolve_tsc_binary || true
        elif [[ -f "$SCRIPT_DIR/package.json" || -f "$SCRIPT_DIR/package-lock.json" ]]; then
            log_step "Trying emitter-local dependencies fallback..."
            if [[ ! -d "$SCRIPT_DIR/node_modules" ]]; then
                log_step "Installing emitter-local dependencies..."
                (cd "$SCRIPT_DIR" && npm install --include=dev --no-fund --no-audit)
            fi
            resolve_tsc_binary || true
        fi
    fi

    if ! resolve_tsc_binary; then
        log_error "TypeScript compiler not found in scripts dependencies."
        log_error "  Tried:"
        log_error "  $scripts_dir/node_modules/.bin/tsc"
        log_error "  $SCRIPT_DIR/node_modules/.bin/tsc"
        die "Install TypeScript in scripts package and retry"
    fi

    (
        cd "$SCRIPT_DIR"
        # Use tsc from scripts or emit fallback node_modules.
        "$TSC_BIN" -p tsconfig.json
    )
    current_head="$(git -C "$ROOT_DIR" rev-parse HEAD 2>/dev/null || true)"
    if [[ -n "$current_head" ]]; then
        mkdir -p "$(dirname "$state_file")"
        printf '%s\n' "$current_head" > "$state_file"
    fi
    log_success "Runner built"
}

# Main
main() {
    local skip_build=0
    local show_help=0
    for arg in "$@"; do
        case "$arg" in
            --skip-build|--no-build) skip_build=1 ;;
            --help|-h) show_help=1 ;;
        esac
    done

    if [[ "$show_help" -eq 1 ]]; then
        print_help
        return 0
    fi

    # Check baselines exist
    local baselines_dir="$ROOT_DIR/TypeScript/tests/baselines/reference"
    if [[ ! -d "$baselines_dir" ]]; then
        die "TypeScript baselines not found. Run: ./scripts/setup-ts-submodule.sh"
    fi

    if [[ "$skip_build" -eq 0 ]]; then
        ensure_tsz_binary
        build_runner
    else
        if ! resolve_tsz_binary; then
            die "tsz binary not found. Run once without --skip-build/--no-build."
        fi
        if [[ ! -f "$SCRIPT_DIR/dist/runner.js" ]]; then
            die "Runner JS not built. Run once without --skip-build/--no-build."
        fi
    fi

    log_step "Running emit tests..."
    echo ""

    cd "$SCRIPT_DIR"
    node dist/runner.js "$@"
}

main "$@"
