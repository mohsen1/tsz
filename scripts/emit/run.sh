#!/usr/bin/env bash
#
# TSZ Emit Test Runner
# ====================
#
# Tests TSZ emit against a fresh pinned TypeScript 7 process observation.
# Checked-in baselines provide sources/directives/product domains, never bytes.
#
# Usage: ./run.sh [options]
#
# Options:
#   --max=N               Maximum tests (default: all)
#   --filter=PATTERN      Filter tests by name
#   --concurrency=N, -jN  Parallel workers (default: CPU count)
#   --timeout=MS          Per-test timeout in ms (default: 5000)
#   --skip-build, --no-build
#                         Skip rebuild checks (set TSZ_BIN if multiple builds exist)
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
    "$ROOT_DIR/crates/tsz-core/src"
    "$ROOT_DIR/crates/tsz-cli/src"
    "$ROOT_DIR/crates/tsz-core/Cargo.toml"
    "$ROOT_DIR/crates/tsz-cli/Cargo.toml"
    "$ROOT_DIR/Cargo.toml"
    "$ROOT_DIR/Cargo.lock"
)
RUNNER_WATCH_PATHS=(
    "$SCRIPT_DIR/src"
    "$SCRIPT_DIR/oracle-manifest.json"
    "$SCRIPT_DIR/tsconfig.json"
    "$SCRIPT_DIR/../package.json"
    "$SCRIPT_DIR/../package-lock.json"
)
ROOT_GIT_AVAILABLE=0
ROOT_GIT_HEAD=""
ROOT_GIT_TSZ_STATUS_CACHED=0
ROOT_GIT_TSZ_STATUS_OUTPUT=""
ROOT_GIT_RUNNER_STATUS_CACHED=0
ROOT_GIT_RUNNER_STATUS_OUTPUT=""
LAST_BUILT_TSZ_BIN=""

if command -v git &>/dev/null && git -C "$ROOT_DIR" rev-parse --is-inside-work-tree &>/dev/null; then
    ROOT_GIT_AVAILABLE=1
    ROOT_GIT_HEAD="$(git -C "$ROOT_DIR" rev-parse HEAD 2>/dev/null || true)"
fi

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
                        Skip rebuild checks for tsz and runner (set TSZ_BIN if multiple builds exist)
  --verbose, -v         Detailed output with diffs
  --js-only             Test JavaScript emit only
  --dts-only            Test declaration emit only
  --json-out[=PATH]     Write machine-readable results JSON (default: emit-detail.json)
  --help, -h            Show this help
EOF
}

resolve_tsc_binary() {
    local scripts_dir
    scripts_dir="$(cd "$SCRIPT_DIR/.." && pwd)"

    local candidates=(
        "$scripts_dir/node_modules/typescript/bin/tsc"
        "$scripts_dir/node_modules/.bin/tsc"
        "$SCRIPT_DIR/node_modules/typescript/bin/tsc"
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

write_state_head() {
    local state_file="$1"
    local head="$2"
    [[ -n "$head" ]] || return 0
    if [[ -f "$state_file" && "$head" == "$(cat "$state_file" 2>/dev/null || true)" ]]; then
        return 0
    fi
    mkdir -p "$(dirname "$state_file")"
    printf '%s\n' "$head" > "$state_file"
}

cache_tsz_watch_status() {
    if [[ "$ROOT_GIT_TSZ_STATUS_CACHED" -eq 1 ]]; then
        return 0
    fi
    ROOT_GIT_TSZ_STATUS_CACHED=1

    if [[ "$ROOT_GIT_AVAILABLE" -ne 1 ]]; then
        ROOT_GIT_TSZ_STATUS_OUTPUT=""
        return 0
    fi

    ROOT_GIT_TSZ_STATUS_OUTPUT="$(git -C "$ROOT_DIR" status --porcelain --short -- "${TSZ_WATCH_PATHS[@]}" 2>/dev/null || true)"
}

cache_runner_watch_status() {
    if [[ "$ROOT_GIT_RUNNER_STATUS_CACHED" -eq 1 ]]; then
        return 0
    fi
    ROOT_GIT_RUNNER_STATUS_CACHED=1

    if [[ "$ROOT_GIT_AVAILABLE" -ne 1 ]]; then
        ROOT_GIT_RUNNER_STATUS_OUTPUT=""
        return 0
    fi

    ROOT_GIT_RUNNER_STATUS_OUTPUT="$(git -C "$ROOT_DIR" status --porcelain --short -- "${RUNNER_WATCH_PATHS[@]}" 2>/dev/null || true)"
}

tsz_watched_changes() {
    cache_tsz_watch_status

    if [[ -n "$ROOT_GIT_TSZ_STATUS_OUTPUT" ]]; then
        return 0
    fi

    return 1
}

runner_watched_changes() {
    cache_runner_watch_status

    if [[ -n "$ROOT_GIT_RUNNER_STATUS_OUTPUT" ]]; then
        return 0
    fi

    return 1
}

state_head_matches() {
    local state_file="$1"
    local head="$2"
    local stored_head

    [[ -f "$state_file" ]] || return 1
    stored_head="$(cat "$state_file" 2>/dev/null || true)"
    [[ "$head" == "$stored_head" ]] || return 1

    return 0
}

# Resolve tsz binary path for the Node runner
resolve_tsz_binary() {
    local reject_ambiguous="${1:-0}"
    local candidates=()

    if [[ -n "${TSZ_BIN:-}" ]]; then
        if [[ ! -x "$TSZ_BIN" ]]; then
            log_error "explicit TSZ_BIN is not executable: $TSZ_BIN"
            return 1
        fi
        TSZ_BIN="$(cd "$(dirname "$TSZ_BIN")" && pwd)/$(basename "$TSZ_BIN")"
        export TSZ_BIN
        log_info "Using explicit tsz binary: $TSZ_BIN"
        return 0
    fi

    if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
        candidates+=("$CARGO_TARGET_DIR/release/tsz")
    fi

    candidates+=(
        "$ROOT_DIR/.target/dist-fast/tsz"
        "$ROOT_DIR/.target/release/tsz"
        "$ROOT_DIR/target/release/tsz"
    )

    local executable_candidates=()
    local tsz_bin resolved existing duplicate
    for tsz_bin in "${candidates[@]}"; do
        if [[ -x "$tsz_bin" ]]; then
            resolved="$(cd "$(dirname "$tsz_bin")" && pwd)/$(basename "$tsz_bin")"
            duplicate=0
            if [[ "${#executable_candidates[@]}" -gt 0 ]]; then
                for existing in "${executable_candidates[@]}"; do
                    if [[ "$existing" == "$resolved" ]]; then
                        duplicate=1
                        break
                    fi
                done
            fi
            if [[ "$duplicate" -eq 0 ]]; then
                executable_candidates+=("$resolved")
            fi
        fi
    done

    if [[ "${#executable_candidates[@]}" -eq 0 ]]; then
        log_error "tsz binary not found in known target directories"
        return 1
    fi

    if [[ "$reject_ambiguous" -eq 1 && "${#executable_candidates[@]}" -gt 1 ]]; then
        local first="${executable_candidates[0]}"
        for existing in "${executable_candidates[@]:1}"; do
            if ! cmp -s "$first" "$existing"; then
                log_error "--skip-build found multiple different tsz binaries:"
                for tsz_bin in "${executable_candidates[@]}"; do
                    log_error "  $tsz_bin"
                done
                log_error "Set TSZ_BIN to the exact binary intended for this measurement."
                return 1
            fi
        done
    fi

    TSZ_BIN="${executable_candidates[0]}"
    export TSZ_BIN
    log_info "Using tsz binary: $TSZ_BIN"
    return 0
}

rebuild_tsz_binary() {
    log_step "Building tsz binary..."
    local target_dir="${CARGO_TARGET_DIR:-.target}"
    (
        cd "$ROOT_DIR"
        CARGO_TARGET_DIR="$target_dir" cargo build --release -p tsz-cli --bin tsz
    )
    if [[ "$target_dir" = /* ]]; then
        LAST_BUILT_TSZ_BIN="$target_dir/release/tsz"
    else
        LAST_BUILT_TSZ_BIN="$ROOT_DIR/$target_dir/release/tsz"
    fi
    log_success "tsz binary built"
}

ensure_tsz_binary() {
    if ! resolve_tsz_binary; then
        rebuild_tsz_binary
        if [[ -n "$LAST_BUILT_TSZ_BIN" && -x "$LAST_BUILT_TSZ_BIN" ]]; then
            TSZ_BIN="$LAST_BUILT_TSZ_BIN"
            export TSZ_BIN
        else
            resolve_tsz_binary || {
                log_error "Failed to resolve tsz binary after build"
                exit 1
            }
        fi
        return 0
    fi

    local tsz_bin="$TSZ_BIN"
    local stale=0
    local state_file="$(dirname "$tsz_bin")/.tsz_binary_head"

    if [[ "$ROOT_GIT_AVAILABLE" -eq 1 ]]; then
        if ! state_head_matches "$state_file" "$ROOT_GIT_HEAD"; then
            stale=1
        elif tsz_watched_changes; then
            stale=1
        fi
    else
        # Fallback when not in a git checkout: use filesystem mtime checks.
        if find "${TSZ_WATCH_PATHS[@]}" -type f -newer "$tsz_bin" 2>/dev/null -print -quit | read -r _; then
            stale=1
        fi
    fi

    if [[ "$stale" -eq 0 ]]; then
        write_state_head "$state_file" "$ROOT_GIT_HEAD"
        return 0
    fi

    if [[ "$stale" -eq 1 ]]; then
        log_info "Detected stale tsz binary; rebuilding"
        rebuild_tsz_binary
        if [[ -n "$LAST_BUILT_TSZ_BIN" && -x "$LAST_BUILT_TSZ_BIN" ]]; then
            TSZ_BIN="$LAST_BUILT_TSZ_BIN"
            export TSZ_BIN
        else
            resolve_tsz_binary || {
                log_error "Failed to resolve tsz binary after rebuild"
                exit 1
            }
        fi
        write_state_head "$(dirname "$TSZ_BIN")/.tsz_binary_head" "$ROOT_GIT_HEAD"
    fi
}

# Build TypeScript runner
build_runner() {
    local dist_runner="$SCRIPT_DIR/dist/runner.js"
    local stale=0
    local state_file="$(dirname "$dist_runner")/.runner_build_head"

    if [[ ! -f "$dist_runner" ]]; then
        stale=1
    else
        if [[ "$ROOT_GIT_AVAILABLE" -eq 1 ]]; then
            if ! state_head_matches "$state_file" "$ROOT_GIT_HEAD"; then
                stale=1
            elif runner_watched_changes; then
                stale=1
            fi
        else
            # Fallback when not in a git checkout: use filesystem mtime checks.
            if find "${RUNNER_WATCH_PATHS[@]}" -type f -newer "$dist_runner" 2>/dev/null -print -quit | read -r _; then
                stale=1
            fi
        fi

        if [[ "$stale" -eq 0 ]]; then
            write_state_head "$state_file" "$ROOT_GIT_HEAD"
            log_success "Runner up to date"
            return 0
        fi
    fi

    log_step "Building emit runner..."
    local scripts_dir
    scripts_dir="$(cd "$SCRIPT_DIR/.." && pwd)"
    # Install/validate the exact native compiler and platform standard libs.
    if ! "$scripts_dir/setup/ensure-pinned-typescript.sh" "$scripts_dir"; then
        die "Install the pinned TypeScript compiler and retry"
    fi
    if ! node -e 'const {createRequire}=require("node:module"); const r=createRequire(process.argv[1]); const jsonc=r("jsonc-parser"); if (typeof jsonc.parse !== "function") process.exit(1)' "$scripts_dir/package.json"; then
        die "Pinned JSONC parser is unavailable in scripts dependencies"
    fi

    # Re-check the legacy emitter-local location when dependencies live there.
    if [[ ! -x "$scripts_dir/node_modules/typescript/bin/tsc" && -d "$SCRIPT_DIR/node_modules" ]]; then
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
        log_error "  $scripts_dir/node_modules/typescript/bin/tsc"
        log_error "  $scripts_dir/node_modules/.bin/tsc"
        log_error "  $SCRIPT_DIR/node_modules/typescript/bin/tsc"
        log_error "  $SCRIPT_DIR/node_modules/.bin/tsc"
        die "Install TypeScript in scripts package and retry"
    fi

    (
        cd "$SCRIPT_DIR"
        # Use tsc from scripts or emit fallback node_modules.
        "$TSC_BIN" -p tsconfig.json
    )
    write_state_head "$state_file" "$ROOT_GIT_HEAD"
    log_success "Runner built"
}

run_harness_contracts() {
    log_step "Verifying canonical emit-harness contracts..."
    python3 -m unittest "$SCRIPT_DIR/test_output_surgery_audit.py"
    python3 "$SCRIPT_DIR/audit-output-surgery.py" --fail-on-warnings
    [[ -f "$SCRIPT_DIR/dist/canonical-truth.test.js" ]] \
        || die "Canonical emit truth test is not built; rerun without --skip-build"

    local test_file
    for test_file in "$SCRIPT_DIR"/dist/*.test.js; do
        [[ -f "$test_file" ]] || die "Compiled emit harness test not found: $test_file"
        node "$test_file"
    done
}

# Main
main() {
    local skip_build=0
    local show_help=0
    local runner_args=()
    for arg in "$@"; do
        case "$arg" in
            --skip-build|--no-build) skip_build=1 ;;
            --help|-h) show_help=1 ;;
            *) runner_args+=("$arg") ;;
        esac
    done

    if [[ "$show_help" -eq 1 ]]; then
        print_help
        return 0
    fi

    # Check baselines exist
    local baselines_dir="$ROOT_DIR/TypeScript/tests/baselines/reference"
    if [[ ! -d "$baselines_dir" ]]; then
        die "TypeScript baselines not found. Run: ./scripts/setup/setup-ts-submodule.sh"
    fi

    if [[ "$skip_build" -eq 0 ]]; then
        ensure_tsz_binary
        build_runner
    else
        if ! resolve_tsz_binary 1; then
            die "tsz binary is missing or ambiguous. Set TSZ_BIN or rerun without --skip-build/--no-build."
        fi
        if [[ ! -f "$SCRIPT_DIR/dist/runner.js" ]]; then
            die "Runner JS not built. Run once without --skip-build/--no-build."
        fi
    fi

    run_harness_contracts
    log_step "Running emit tests..."
    echo ""

    cd "$SCRIPT_DIR"
    if [[ "${#runner_args[@]}" -gt 0 ]]; then
        node dist/runner.js "${runner_args[@]}"
    else
        node dist/runner.js
    fi
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    main "$@"
fi
