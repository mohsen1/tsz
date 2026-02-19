#!/usr/bin/env bash
#
# Benchmark: tsz vs tsgo (TypeScript 7 / typescript-go)
#
# Compares compilation performance across various file sizes and complexities.
# Requires: hyperfine (brew install hyperfine)
# tsgo is auto-installed locally (pinned) unless TSGO is explicitly provided.
#
# Usage:
#   ./scripts/bench-vs-tsgo.sh                    # Full benchmark suite
#   ./scripts/bench-vs-tsgo.sh --quick            # Quick smoke test (fewer runs, fewer files)
#   ./scripts/bench-vs-tsgo.sh --json             # Export results to JSON
#   ./scripts/bench-vs-tsgo.sh --filter 'BCT|CFA' # Run only tests matching regex
#   ./scripts/bench-vs-tsgo.sh --filter 'utility-types' # Run only utility-types benchmarks
#   ./scripts/bench-vs-tsgo.sh --rebuild          # Force rebuild of optimized binary
#
# The benchmark uses an isolated target directory (.target-bench/) to prevent
# interference from other cargo builds. The binary is built with the 'dist' profile
# which enables maximum optimizations (LTO=fat, codegen-units=1, stripped symbols).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Default lib assets for fresh checkouts (tsz expects a lib directory)
TSZ_LIB_DIR_DEFAULT="$PROJECT_ROOT/src/lib-assets"
TSZ_LIB_DIR="${TSZ_LIB_DIR:-$TSZ_LIB_DIR_DEFAULT}"
export TSZ_LIB_DIR

# Dedicated target directory for benchmarks - isolated from dev builds
# This prevents other cargo builds from accidentally overwriting the optimized binary
BENCH_TARGET_DIR="$PROJECT_ROOT/.target-bench"

# Compilers
TSZ="$BENCH_TARGET_DIR/dist/tsz"
TSGO="${TSGO:-}"
TSGO_TOOL_DIR="${TSGO_TOOL_DIR:-$BENCH_TARGET_DIR/tools/tsgo}"
TSGO_LOCAL_BIN="$TSGO_TOOL_DIR/node_modules/.bin/tsgo"
# tsc (TypeScript reference compiler)
TSC="${TSC:-}"
TSC_TOOL_DIR="${TSC_TOOL_DIR:-$BENCH_TARGET_DIR/tools/tsc}"
TSC_LOCAL_BIN="$TSC_TOOL_DIR/node_modules/.bin/tsc"
# pinned tsgo package for reproducible benchmark runs
TSGO_NPM_SPEC="${TSGO_NPM_SPEC:-@typescript/native-preview@7.0.0-dev.20260206.1}"
TSC_NPM_SPEC="${TSC_NPM_SPEC:-}"

# External benchmark fixtures (not checked into git)
EXTERNAL_BENCH_DIR="${EXTERNAL_BENCH_DIR:-$BENCH_TARGET_DIR/external}"
UTILITY_TYPES_REPO="${UTILITY_TYPES_REPO:-https://github.com/piotrwitek/utility-types.git}"
# pinned to v3.11.0 commit for reproducible benchmarks
UTILITY_TYPES_REF="${UTILITY_TYPES_REF:-2ee1f6ecb241651ab22390fee7ee5349942efda2}"
UTILITY_TYPES_DIR="$EXTERNAL_BENCH_DIR/utility-types"
TS_TOOLBELT_REPO="${TS_TOOLBELT_REPO:-https://github.com/millsp/ts-toolbelt.git}"
# pinned commit for reproducible benchmarks
TS_TOOLBELT_REF="${TS_TOOLBELT_REF:-b8a49285e3ed3a7d8bb8e0b433389eac46a5f140}"
TS_TOOLBELT_DIR="$EXTERNAL_BENCH_DIR/ts-toolbelt"
TS_ESSENTIALS_REPO="${TS_ESSENTIALS_REPO:-https://github.com/ts-essentials/ts-essentials.git}"
# pinned commit for reproducible benchmarks
TS_ESSENTIALS_REF="${TS_ESSENTIALS_REF:-5abe8700b42068048bd3c368e0531b6defe56558}"
TS_ESSENTIALS_DIR="$EXTERNAL_BENCH_DIR/ts-essentials"
NEXTJS_REPO="${NEXTJS_REPO:-https://github.com/vercel/next.js.git}"
# pinned canary commit for reproducible benchmarks
NEXTJS_REF="${NEXTJS_REF:-09851e208cc62c8b6fe7a953b42c88e843129178}"
NEXTJS_DIR="$EXTERNAL_BENCH_DIR/next.js"

# Parse arguments
QUICK_MODE=false
JSON_OUTPUT=false
JSON_FILE=""
FILTER=""
FORCE_REBUILD=false
NEXTJS_BENCHMARK_ENABLED="${NEXTJS_BENCHMARK_ENABLED:-0}"
while [[ $# -gt 0 ]]; do
    case $1 in
        --quick) QUICK_MODE=true; shift ;;
        --json) JSON_OUTPUT=true; shift ;;
        --json-file) JSON_OUTPUT=true; JSON_FILE="$2"; shift 2 ;;
        --filter) FILTER="$2"; shift 2 ;;
        --rebuild) FORCE_REBUILD=true; shift ;;
        --help|-h)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --quick     Quick smoke test (fewer runs, fewer files)"
            echo "  --json      Export results to JSON (default path: artifacts/bench-vs-tsgo-<timestamp>.json)"
            echo "  --json-file Write JSON results to a specific path"
            echo "  --filter    Run only tests matching regex (e.g., --filter 'BCT|CFA')"
            echo "  --rebuild   Force rebuild of tsz binary (ensures fresh optimized build)"
            echo "  --help      Show this help"
            echo ""
            echo "The benchmark uses an isolated target directory (.target-bench/) to prevent"
            echo "interference from other cargo builds."
            echo ""
            echo "Environment overrides:"
            echo "  TSGO=<path>            Use a specific tsgo binary (skip auto-install)"
            echo "  TSGO_NPM_SPEC=<spec>   Override pinned npm package (default: $TSGO_NPM_SPEC)"
            echo "  TSC=<path>             Use a specific tsc binary (skip auto-install)"
            echo "  TSC_NPM_SPEC=<spec>    Override pinned typescript npm version"
            echo "  TSZ_LIB_DIR=<path>     Override tsz lib assets (default: $TSZ_LIB_DIR_DEFAULT)"
            echo "  UTILITY_TYPES_REF=<sha> Override pinned utility-types commit"
            echo "  TS_TOOLBELT_REF=<sha>  Override pinned ts-toolbelt commit"
            echo "  TS_ESSENTIALS_REF=<sha> Override pinned ts-essentials commit"
            echo "  NEXTJS_REF=<sha>       Override pinned next.js commit"
            exit 0
            ;;
        *) shift ;;
    esac
done

# Benchmark settings
if [ "$QUICK_MODE" = true ]; then
    WARMUP=1
    MIN_RUNS=3
    MAX_RUNS=5
    echo "Quick mode: fewer runs, subset of files"
else
    WARMUP=3
    MIN_RUNS=10
    MAX_RUNS=50
fi

if [ -n "$FILTER" ]; then
    echo "Filter: only running tests matching /$FILTER/"
fi

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

print_header() {
    echo
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BOLD}  $1${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

print_subheader() {
    echo
    echo -e "${CYAN}▶ $1${NC}"
    echo -e "${CYAN}─────────────────────────────────────────────────────────────────────────────${NC}"
}

file_info() {
    local file="$1"
    local lines=$(wc -l < "$file" 2>/dev/null | tr -d ' ')
    local bytes=$(wc -c < "$file" 2>/dev/null | tr -d ' ')
    local kb=$((bytes / 1024))
    echo "${lines} lines, ${kb}KB"
}

ensure_tsgo() {
    # Honor explicit TSGO override when provided by caller.
    if [ -n "$TSGO" ]; then
        if [ ! -x "$TSGO" ]; then
            echo -e "${RED}✗ TSGO is set but not executable: $TSGO${NC}"
            exit 1
        fi
        return
    fi

    if ! command -v npm &>/dev/null; then
        echo -e "${RED}✗ npm not found${NC}"
        echo "  npm is required to auto-install tsgo ($TSGO_NPM_SPEC)"
        exit 1
    fi

    mkdir -p "$TSGO_TOOL_DIR"
    local spec_file="$TSGO_TOOL_DIR/.tsgo-spec"
    local installed_spec=""
    if [ -f "$spec_file" ]; then
        installed_spec="$(cat "$spec_file")"
    fi

    if [ ! -x "$TSGO_LOCAL_BIN" ] || [ "$installed_spec" != "$TSGO_NPM_SPEC" ]; then
        echo -e "${CYAN}Installing tsgo locally (${TSGO_NPM_SPEC})...${NC}"
        npm install \
            --prefix "$TSGO_TOOL_DIR" \
            --no-audit \
            --no-fund \
            --loglevel=error \
            "$TSGO_NPM_SPEC" >/dev/null
        printf '%s\n' "$TSGO_NPM_SPEC" > "$spec_file"
    fi

    if [ ! -x "$TSGO_LOCAL_BIN" ]; then
        echo -e "${RED}✗ tsgo install failed: binary not found at $TSGO_LOCAL_BIN${NC}"
        exit 1
    fi

    TSGO="$TSGO_LOCAL_BIN"
}

resolve_tsc_npm_spec() {
    local sha=""
    if [ -d "$PROJECT_ROOT/TypeScript" ]; then
        sha="$(git -C "$PROJECT_ROOT/TypeScript" rev-parse HEAD 2>/dev/null || echo "")"
    fi

    if [ -z "$sha" ]; then
        echo ""
        return
    fi

    node -e "const v=require('./scripts/typescript-versions.json'); const sha=process.argv[1]; const m=v.mappings?.[sha]; console.log(m?.npm || v.default?.npm || '');" "$sha"
}

ensure_tsc() {
    # Honor explicit TSC override when provided by caller.
    if [ -n "$TSC" ]; then
        if [ ! -x "$TSC" ]; then
            echo -e "${RED}✗ TSC is set but not executable: $TSC${NC}"
            exit 1
        fi
        return
    fi

    if ! command -v npm &>/dev/null; then
        echo -e "${RED}✗ npm not found${NC}"
        echo "  npm is required to auto-install tsc"
        exit 1
    fi

    local resolved_spec="$TSC_NPM_SPEC"
    if [ -z "$resolved_spec" ]; then
        resolved_spec="$(resolve_tsc_npm_spec)"
    fi
    if [ -z "$resolved_spec" ]; then
        echo -e "${RED}✗ Unable to resolve tsc npm spec from TypeScript submodule${NC}"
        echo "  Set TSC_NPM_SPEC or ensure the TypeScript submodule is present."
        exit 1
    fi

    mkdir -p "$TSC_TOOL_DIR"
    local spec_file="$TSC_TOOL_DIR/.tsc-spec"
    local installed_spec=""
    if [ -f "$spec_file" ]; then
        installed_spec="$(cat "$spec_file")"
    fi

    if [ ! -x "$TSC_LOCAL_BIN" ] || [ "$installed_spec" != "$resolved_spec" ]; then
        echo -e "${CYAN}Installing tsc locally (${resolved_spec})...${NC}"
        npm install \
            --prefix "$TSC_TOOL_DIR" \
            --no-audit \
            --no-fund \
            --loglevel=error \
            "typescript@${resolved_spec}" >/dev/null
        printf '%s\n' "$resolved_spec" > "$spec_file"
    fi

    if [ ! -x "$TSC_LOCAL_BIN" ]; then
        echo -e "${RED}✗ tsc install failed: binary not found at $TSC_LOCAL_BIN${NC}"
        exit 1
    fi

    TSC="$TSC_LOCAL_BIN"
}

check_prerequisites() {
    print_header "Prerequisites Check"
    
    # Check hyperfine
    if ! command -v hyperfine &>/dev/null; then
        echo -e "${RED}✗ hyperfine not found${NC}"
        echo "  Install with: brew install hyperfine"
        exit 1
    fi
    echo -e "${GREEN}✓${NC} hyperfine $(hyperfine --version | head -1)"
    
    # Check jq (optional, for results table)
    if command -v jq &>/dev/null; then
        echo -e "${GREEN}✓${NC} jq $(jq --version)"
    else
        echo -e "${YELLOW}○${NC} jq not found (optional, install for results table)"
    fi
    
    # Check for lib assets directory used by tsz
    if [ ! -d "$TSZ_LIB_DIR" ]; then
        echo -e "${RED}✗ lib directory not found: $TSZ_LIB_DIR${NC}"
        echo "  Set TSZ_LIB_DIR or ensure src/lib-assets exists."
        exit 1
    fi
    echo -e "${GREEN}✓${NC} tsz lib assets: $TSZ_LIB_DIR"

    # Check/build tsz with dedicated benchmark target directory
    # Using isolated target dir prevents other cargo builds from affecting benchmark binary
    local need_rebuild=false
    
    if [ "$FORCE_REBUILD" = true ]; then
        echo -e "${YELLOW}Force rebuild requested...${NC}"
        need_rebuild=true
    elif [ ! -x "$TSZ" ]; then
        echo -e "${YELLOW}Binary not found, building...${NC}"
        need_rebuild=true
    else
        # Verify binary is recent (rebuilt if any Rust source in the workspace
        # changed since the last benchmark build).
        local newest_src
        newest_src="$(find "$PROJECT_ROOT" \
            \( -path "$BENCH_TARGET_DIR" -o -path "$PROJECT_ROOT/.git" \) -prune -o \
            -type f -name "*.rs" -newer "$TSZ" -print -quit 2>/dev/null)"
        if [ -n "$newest_src" ]; then
            echo -e "${YELLOW}Source changed since last build, rebuilding...${NC}"
            need_rebuild=true
        fi
    fi
    
    if [ "$need_rebuild" = true ]; then
        echo -e "${CYAN}Building tsz with dist profile (LTO=fat, codegen-units=1)${NC}"
        echo -e "${CYAN}Target directory: $BENCH_TARGET_DIR${NC}"
        (cd "$PROJECT_ROOT" && CARGO_TARGET_DIR="$BENCH_TARGET_DIR" cargo build --profile dist -p tsz-cli)
    fi
    
    echo -e "${GREEN}✓${NC} tsz: $($TSZ --version 2>&1 | head -1)"
    echo -e "   Binary: $TSZ"
    echo -e "   Size: $(ls -lh "$TSZ" | awk '{print $5}')"
    echo -e "   Built: $(stat -f '%Sm' -t '%Y-%m-%d %H:%M:%S' "$TSZ" 2>/dev/null || stat -c '%y' "$TSZ" 2>/dev/null | cut -d. -f1)"
    
    # Check/install tsgo
    ensure_tsgo
    echo -e "${GREEN}✓${NC} tsgo: $($TSGO --version 2>&1 | head -1)"
    echo -e "   Binary: $TSGO"

    # Check/install tsc
    ensure_tsc
    echo -e "${GREEN}✓${NC} tsc: $($TSC --version 2>&1 | head -1)"
    echo -e "   Binary: $TSC"
}

RESULTS_CSV=""
BENCHMARKS_RUN=0

run_benchmark() {
    local name="$1"
    local file="$2"
    local extra_args="${3:-}"

    # Skip if filter is set and name doesn't match
    if [ -n "$FILTER" ] && ! echo "$name" | grep -qE "$FILTER"; then
        return
    fi

    BENCHMARKS_RUN=$((BENCHMARKS_RUN + 1))

    local lines=$(wc -l < "$file" 2>/dev/null | tr -d ' ')
    local bytes=$(wc -c < "$file" 2>/dev/null | tr -d ' ')
    local kb=$((bytes / 1024))
    local info="${lines} lines, ${kb}KB"

    # Benchmark fixtures must be valid TypeScript for the reference compiler.
    # If tsc fails, treat the fixture as invalid benchmark input and skip it.
    local tsc_check=$($TSC --noEmit $extra_args "$file" >/dev/null 2>&1; echo $?)
    if [ "$tsc_check" -ne 0 ]; then
        local tsc_error=$($TSC --noEmit $extra_args "$file" 2>&1 | head -1)
        echo -e "${YELLOW}$name${NC} - ${YELLOW}SKIP${NC} (tsc fixture error)"
        echo -e "  ${CYAN}tsc error:${NC} $tsc_error" >&2
        return
    fi

    # Pre-validate: record errors in summary table instead of skipping
    local tsz_check=$(TSZ_LIB_DIR="$TSZ_LIB_DIR" $TSZ --noEmit $extra_args "$file" >/dev/null 2>&1; echo $?)
    local tsgo_check=$($TSGO --noEmit $extra_args "$file" >/dev/null 2>&1; echo $?)

    if [ "$tsz_check" -ne 0 ] || [ "$tsgo_check" -ne 0 ]; then
        local status=""
        local tsz_ms="N/A"
        local tsgo_ms="N/A"
        local tsz_lps="N/A"
        local tsgo_lps="N/A"
        local winner="error"
        local ratio="0"

        echo -e "${YELLOW}$name${NC} - ${RED}ERROR${NC}"

        if [ "$tsz_check" -ne 0 ]; then
            status="tsz error"
            tsz_ms="ERR"
            local tsz_error=$(TSZ_LIB_DIR="$TSZ_LIB_DIR" $TSZ --noEmit $extra_args "$file" 2>&1 | head -1)
            echo -e "  ${CYAN}tsz error:${NC} $tsz_error" >&2
        fi

        if [ "$tsgo_check" -ne 0 ]; then
            status="${status:+${status}; }tsgo error"
            tsgo_ms="ERR"
            local tsgo_error=$($TSGO --noEmit $extra_args "$file" 2>&1 | head -1)
            echo -e "  ${CYAN}tsgo error:${NC} $tsgo_error" >&2
        fi

        status="${status:+${status}; }tsc ok"

        RESULTS_CSV="${RESULTS_CSV}${name},${lines},${kb},${tsz_ms},${tsgo_ms},${tsz_lps},${tsgo_lps},${winner},${ratio},${status}\n"
        return
    fi

    echo -e "${GREEN}$name${NC} ($info)"

    # Run benchmark and capture JSON output
    local json_file=$(mktemp)
    if ! hyperfine \
        --warmup "$WARMUP" \
        --min-runs "$MIN_RUNS" \
        --max-runs "$MAX_RUNS" \
        --style full \
        --export-json "$json_file" \
        -n "tsz" "TSZ_LIB_DIR=$TSZ_LIB_DIR $TSZ --noEmit $extra_args $file 2>/dev/null" \
        -n "tsgo" "$TSGO --noEmit $extra_args $file 2>/dev/null"; then
        local status="hyperfine error"
        RESULTS_CSV="${RESULTS_CSV}${name},${lines},${kb},ERR,ERR,N/A,N/A,error,0,${status}\n"
        rm -f "$json_file"
        return
    fi
    
    # Extract times and calculate throughput
    if [ -f "$json_file" ] && command -v jq &>/dev/null; then
        local tsz_mean=$(jq -r '.results[] | select(.command | contains("tsz")) | .mean' "$json_file" 2>/dev/null || echo "0")
        local tsgo_mean=$(jq -r '.results[] | select(.command | contains("tsgo")) | .mean' "$json_file" 2>/dev/null || echo "0")
        
        if [ -n "$tsz_mean" ] && [ -n "$tsgo_mean" ] && [ "$tsz_mean" != "0" ] && [ "$tsgo_mean" != "0" ]; then
            # Calculate throughput (lines/sec) and format times (2 decimal places)
            local tsz_lps=$(printf "%.0f" "$(echo "$lines / $tsz_mean" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            local tsgo_lps=$(printf "%.0f" "$(echo "$lines / $tsgo_mean" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            local tsz_ms=$(printf "%.2f" "$(echo "$tsz_mean * 1000" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            local tsgo_ms=$(printf "%.2f" "$(echo "$tsgo_mean * 1000" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            
            # Determine winner and calculate speedup ratio
            local winner="tsgo"
            local ratio
            if (( $(echo "$tsz_mean < $tsgo_mean" | bc -l) )); then
                winner="tsz"
                ratio=$(printf "%.2f" "$(echo "$tsgo_mean / $tsz_mean" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            else
                ratio=$(printf "%.2f" "$(echo "$tsz_mean / $tsgo_mean" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            fi
            
            RESULTS_CSV="${RESULTS_CSV}${name},${lines},${kb},${tsz_ms},${tsgo_ms},${tsz_lps},${tsgo_lps},${winner},${ratio},\n"
        fi
    fi
    rm -f "$json_file"
}

run_project_benchmark() {
    local name="$1"
    local tsconfig="$2"
    local src_dir="$3"

    # Skip if filter is set and name doesn't match
    if [ -n "$FILTER" ] && ! echo "$name" | grep -qE "$FILTER"; then
        return
    fi

    BENCHMARKS_RUN=$((BENCHMARKS_RUN + 1))

    # Count total TS/TSX source lines in the project
    local lines=$(find "$src_dir" \( -name '*.ts' -o -name '*.tsx' \) -print0 2>/dev/null \
        | xargs -0 wc -l 2>/dev/null | tail -1 | awk '{print $1}')
    local bytes=$(find "$src_dir" \( -name '*.ts' -o -name '*.tsx' \) -print0 2>/dev/null \
        | xargs -0 cat 2>/dev/null | wc -c | tr -d ' ')
    local kb=$((bytes / 1024))
    local info="${lines} lines, ${kb}KB (project)"

    # For project fixtures (except nextjs, which is currently tsgo-only), require
    # a clean tsc pass before benchmarking.
    if [ "$name" != "nextjs" ]; then
        local tsc_check=$($TSC --noEmit -p "$tsconfig" >/dev/null 2>&1; echo $?)
        if [ "$tsc_check" -ne 0 ]; then
            local tsc_error=$($TSC --noEmit -p "$tsconfig" 2>&1 | head -1)
            echo -e "${YELLOW}$name${NC} - ${YELLOW}SKIP${NC} (tsc fixture error)"
            echo -e "  ${CYAN}tsc error:${NC} $tsc_error" >&2
            return
        fi
    fi

    # Pre-validate: record errors in summary table instead of skipping
    local tsz_check=$(TSZ_LIB_DIR="$TSZ_LIB_DIR" $TSZ --noEmit -p "$tsconfig" >/dev/null 2>&1; echo $?)
    local tsgo_check=$($TSGO --noEmit -p "$tsconfig" >/dev/null 2>&1; echo $?)

    if [ "$tsz_check" -ne 0 ] || [ "$tsgo_check" -ne 0 ]; then
        local status=""
        local tsz_ms="N/A"
        local tsgo_ms="N/A"
        local tsz_lps="N/A"
        local tsgo_lps="N/A"
        local winner="error"
        local ratio="0"

        echo -e "${YELLOW}$name${NC} - ${RED}ERROR${NC}"

        if [ "$tsz_check" -ne 0 ]; then
            status="tsz error"
            tsz_ms="ERR"
            local tsz_error=$(TSZ_LIB_DIR="$TSZ_LIB_DIR" $TSZ --noEmit -p "$tsconfig" 2>&1 | head -1)
            echo -e "  ${CYAN}tsz error:${NC} $tsz_error" >&2
        fi

        if [ "$tsgo_check" -ne 0 ]; then
            status="${status:+${status}; }tsgo error"
            tsgo_ms="ERR"
            local tsgo_error=$($TSGO --noEmit -p "$tsconfig" 2>&1 | head -1)
            echo -e "  ${CYAN}tsgo error:${NC} $tsgo_error" >&2
        fi

        if [ "$name" != "nextjs" ]; then
            status="${status:+${status}; }tsc ok"
        fi

        RESULTS_CSV="${RESULTS_CSV}${name},${lines},${kb},${tsz_ms},${tsgo_ms},${tsz_lps},${tsgo_lps},${winner},${ratio},${status}\n"
        return
    fi

    echo -e "${GREEN}$name${NC} ($info)"

    # Run benchmark with -p (project mode)
    local json_file=$(mktemp)
    if ! hyperfine \
        --warmup "$WARMUP" \
        --min-runs "$MIN_RUNS" \
        --max-runs "$MAX_RUNS" \
        --style full \
        --export-json "$json_file" \
        -n "tsz" "TSZ_LIB_DIR=$TSZ_LIB_DIR $TSZ --noEmit -p $tsconfig 2>/dev/null" \
        -n "tsgo" "$TSGO --noEmit -p $tsconfig 2>/dev/null"; then
        local status="hyperfine error"
        RESULTS_CSV="${RESULTS_CSV}${name},${lines},${kb},ERR,ERR,N/A,N/A,error,0,${status}\n"
        rm -f "$json_file"
        return
    fi

    # Extract times and calculate throughput
    if [ -f "$json_file" ] && command -v jq &>/dev/null; then
        local tsz_mean=$(jq -r '.results[] | select(.command | contains("tsz")) | .mean' "$json_file" 2>/dev/null || echo "0")
        local tsgo_mean=$(jq -r '.results[] | select(.command | contains("tsgo")) | .mean' "$json_file" 2>/dev/null || echo "0")

        if [ -n "$tsz_mean" ] && [ -n "$tsgo_mean" ] && [ "$tsz_mean" != "0" ] && [ "$tsgo_mean" != "0" ]; then
            local tsz_lps=$(printf "%.0f" "$(echo "$lines / $tsz_mean" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            local tsgo_lps=$(printf "%.0f" "$(echo "$lines / $tsgo_mean" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            local tsz_ms=$(printf "%.2f" "$(echo "$tsz_mean * 1000" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            local tsgo_ms=$(printf "%.2f" "$(echo "$tsgo_mean * 1000" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")

            local winner="tsgo"
            local ratio
            if (( $(echo "$tsz_mean < $tsgo_mean" | bc -l) )); then
                winner="tsz"
                ratio=$(printf "%.2f" "$(echo "$tsgo_mean / $tsz_mean" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            else
                ratio=$(printf "%.2f" "$(echo "$tsz_mean / $tsgo_mean" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            fi

            RESULTS_CSV="${RESULTS_CSV}${name},${lines},${kb},${tsz_ms},${tsgo_ms},${tsz_lps},${tsgo_lps},${winner},${ratio},\n"
        fi
    fi
    rm -f "$json_file"
}

export_results_json() {
    [ "$JSON_OUTPUT" != true ] && return
    [ -z "$RESULTS_CSV" ] && return

    local default_file="$PROJECT_ROOT/artifacts/bench-vs-tsgo-$(date +%Y%m%d-%H%M%S).json"
    local out_file="${JSON_FILE:-$default_file}"
    mkdir -p "$(dirname "$out_file")"

    local expanded_csv
    expanded_csv="$(echo -e "$RESULTS_CSV")"

    RESULTS_CSV_EXPANDED="$expanded_csv" \
    QUICK_MODE_VALUE="$QUICK_MODE" \
    FILTER_VALUE="$FILTER" \
    TSZ_BIN_VALUE="$TSZ" \
    TSGO_BIN_VALUE="$TSGO" \
    TSC_BIN_VALUE="$TSC" \
    BENCHMARKS_RUN_VALUE="$BENCHMARKS_RUN" \
    node - "$out_file" <<'NODE'
const fs = require("node:fs");
const outFile = process.argv[2];

const csv = process.env.RESULTS_CSV_EXPANDED || "";
const rows = csv
  .split(/\r?\n/)
  .map((line) => line.trim())
  .filter(Boolean)
  .map((line) => {
    const parts = line.split(",");
    while (parts.length < 10) parts.push("");
    const [name, lines, kb, tszMs, tsgoMs, tszLps, tsgoLps, winner, factor, status] = parts;
    const toNumber = (value) => {
      if (!value || value === "N/A" || value === "ERR") return null;
      const parsed = Number(value);
      return Number.isFinite(parsed) ? parsed : null;
    };
    return {
      name,
      lines: toNumber(lines),
      kb: toNumber(kb),
      tsz_ms: toNumber(tszMs),
      tsgo_ms: toNumber(tsgoMs),
      tsz_lps: toNumber(tszLps),
      tsgo_lps: toNumber(tsgoLps),
      winner: winner || null,
      factor: toNumber(factor),
      status: status || null,
    };
  });

const tszWins = rows.filter((row) => row.winner === "tsz").length;
const tsgoWins = rows.filter((row) => row.winner === "tsgo").length;
const errorCases = rows.filter((row) => row.status).length;

const payload = {
  generated_at: new Date().toISOString(),
  benchmark_runner: "scripts/bench-vs-tsgo.sh",
  quick_mode: process.env.QUICK_MODE_VALUE === "true",
  filter: process.env.FILTER_VALUE || null,
  binaries: {
    tsz: process.env.TSZ_BIN_VALUE || null,
    tsgo: process.env.TSGO_BIN_VALUE || null,
    tsc: process.env.TSC_BIN_VALUE || null,
  },
  totals: {
    benchmarks_run: Number(process.env.BENCHMARKS_RUN_VALUE || rows.length),
    rows: rows.length,
    tsz_wins: tszWins,
    tsgo_wins: tsgoWins,
    error_cases: errorCases,
  },
  results: rows,
};

fs.writeFileSync(outFile, `${JSON.stringify(payload, null, 2)}\n`, "utf8");
NODE

    echo -e "${GREEN}JSON results written:${NC} $out_file"
}

is_benchmark_selected() {
    local name="$1"
    if [ -z "$FILTER" ]; then
        return 0
    fi
    echo "$name" | grep -qE "$FILTER"
}

ensure_nextjs_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"

    if [ ! -d "$NEXTJS_DIR/.git" ]; then
        echo -e "${CYAN}Cloning next.js with sparse checkout (packages/next only)...${NC}"
        git init --quiet "$NEXTJS_DIR"
        git -C "$NEXTJS_DIR" remote add origin "$NEXTJS_REPO"
        # --no-cone allows mixing individual root files with directory patterns
        # packages/next/tsconfig.json extends ../../tsconfig-tsec.json
        git -C "$NEXTJS_DIR" sparse-checkout init --no-cone
        git -C "$NEXTJS_DIR" sparse-checkout set \
            '/tsconfig-tsec.json' \
            '/packages/next/tsconfig.json' \
            '/packages/next/src/'
        git -C "$NEXTJS_DIR" fetch --quiet --depth 1 origin "$NEXTJS_REF"
        git -C "$NEXTJS_DIR" checkout --quiet FETCH_HEAD
    fi

    local current_ref
    current_ref="$(git -C "$NEXTJS_DIR" rev-parse HEAD 2>/dev/null || echo "")"
    if [ "$current_ref" != "$NEXTJS_REF" ]; then
        echo -e "${CYAN}Pinning next.js to ${NEXTJS_REF:0:12}...${NC}"
        git -C "$NEXTJS_DIR" fetch --quiet --depth 1 origin "$NEXTJS_REF"
        git -C "$NEXTJS_DIR" checkout --quiet FETCH_HEAD
    fi
}

ensure_utility_types_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"

    if [ ! -d "$UTILITY_TYPES_DIR/.git" ]; then
        echo -e "${CYAN}Cloning utility-types fixture...${NC}"
        git clone --quiet --no-tags --depth 1 "$UTILITY_TYPES_REPO" "$UTILITY_TYPES_DIR"
    fi

    # If users modified the local fixture, reclone to keep benchmarks deterministic
    if [ -n "$(git -C "$UTILITY_TYPES_DIR" status --porcelain 2>/dev/null)" ]; then
        echo -e "${YELLOW}utility-types fixture is dirty; recloning for reproducibility...${NC}"
        rm -rf "$UTILITY_TYPES_DIR"
        git clone --quiet --no-tags --depth 1 "$UTILITY_TYPES_REPO" "$UTILITY_TYPES_DIR"
    fi

    local current_ref
    current_ref="$(git -C "$UTILITY_TYPES_DIR" rev-parse HEAD 2>/dev/null || echo "")"
    if [ "$current_ref" != "$UTILITY_TYPES_REF" ]; then
        echo -e "${CYAN}Pinning utility-types to ${UTILITY_TYPES_REF:0:12}...${NC}"
        git -C "$UTILITY_TYPES_DIR" fetch --quiet --depth 1 origin "$UTILITY_TYPES_REF"
        git -C "$UTILITY_TYPES_DIR" checkout --quiet --detach FETCH_HEAD
    fi
}

ensure_ts_toolbelt_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"

    if [ ! -d "$TS_TOOLBELT_DIR/.git" ]; then
        echo -e "${CYAN}Cloning ts-toolbelt fixture...${NC}"
        git clone --quiet --no-tags --depth 1 "$TS_TOOLBELT_REPO" "$TS_TOOLBELT_DIR"
    fi

    if [ -n "$(git -C "$TS_TOOLBELT_DIR" status --porcelain 2>/dev/null)" ]; then
        echo -e "${YELLOW}ts-toolbelt fixture is dirty; recloning for reproducibility...${NC}"
        rm -rf "$TS_TOOLBELT_DIR"
        git clone --quiet --no-tags --depth 1 "$TS_TOOLBELT_REPO" "$TS_TOOLBELT_DIR"
    fi

    local current_ref
    current_ref="$(git -C "$TS_TOOLBELT_DIR" rev-parse HEAD 2>/dev/null || echo "")"
    if [ "$current_ref" != "$TS_TOOLBELT_REF" ]; then
        echo -e "${CYAN}Pinning ts-toolbelt to ${TS_TOOLBELT_REF:0:12}...${NC}"
        git -C "$TS_TOOLBELT_DIR" fetch --quiet --depth 1 origin "$TS_TOOLBELT_REF"
        git -C "$TS_TOOLBELT_DIR" checkout --quiet --detach FETCH_HEAD
    fi
}

ensure_ts_essentials_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"

    if [ ! -d "$TS_ESSENTIALS_DIR/.git" ]; then
        echo -e "${CYAN}Cloning ts-essentials fixture...${NC}"
        git clone --quiet --no-tags --depth 1 "$TS_ESSENTIALS_REPO" "$TS_ESSENTIALS_DIR"
    fi

    if [ -n "$(git -C "$TS_ESSENTIALS_DIR" status --porcelain 2>/dev/null)" ]; then
        echo -e "${YELLOW}ts-essentials fixture is dirty; recloning for reproducibility...${NC}"
        rm -rf "$TS_ESSENTIALS_DIR"
        git clone --quiet --no-tags --depth 1 "$TS_ESSENTIALS_REPO" "$TS_ESSENTIALS_DIR"
    fi

    local current_ref
    current_ref="$(git -C "$TS_ESSENTIALS_DIR" rev-parse HEAD 2>/dev/null || echo "")"
    if [ "$current_ref" != "$TS_ESSENTIALS_REF" ]; then
        echo -e "${CYAN}Pinning ts-essentials to ${TS_ESSENTIALS_REF:0:12}...${NC}"
        git -C "$TS_ESSENTIALS_DIR" fetch --quiet --depth 1 origin "$TS_ESSENTIALS_REF"
        git -C "$TS_ESSENTIALS_DIR" checkout --quiet --detach FETCH_HEAD
    fi
}

run_utility_types_benchmarks() {
    local benchmark_names=(
        "utility-types/index.ts"
        "utility-types/utility-types.ts"
        "utility-types/mapped-types.ts"
        "utility-types/aliases-and-guards.ts"
    )

    local should_run=false
    local name
    for name in "${benchmark_names[@]}"; do
        if is_benchmark_selected "$name"; then
            should_run=true
            break
        fi
    done

    if [ "$should_run" != true ]; then
        return
    fi

    print_header "Real-world External Library - utility-types"
    ensure_utility_types_fixture
    echo -e "${GREEN}✓${NC} utility-types pinned at $(git -C "$UTILITY_TYPES_DIR" rev-parse --short HEAD)"

    # Use project's tsconfig lib settings (dom, es2017) for fair comparison
    # Without this, tsz loads all default libs which is slower and doesn't match tsgo's behavior
    local lib_args="--lib dom,es2017"

    local files
    if [ "$QUICK_MODE" = true ]; then
        files=("src/index.ts")
    else
        files=(
            "src/index.ts"
            "src/utility-types.ts"
            "src/mapped-types.ts"
            "src/aliases-and-guards.ts"
        )
    fi

    local rel
    for rel in "${files[@]}"; do
        local full_path="$UTILITY_TYPES_DIR/$rel"
        if [ -f "$full_path" ]; then
            run_benchmark "utility-types/${rel#src/}" "$full_path" "$lib_args"
            echo
        fi
    done
}

run_ts_toolbelt_benchmarks() {
    local benchmark_names=(
        "ts-toolbelt/Iteration/Iteration.ts"
        "ts-toolbelt/Misc/BuiltIn.ts"
        "ts-toolbelt/Object/Invert.ts"
        "ts-toolbelt/Any/Compute.ts"
    )

    local should_run=false
    local name
    for name in "${benchmark_names[@]}"; do
        if is_benchmark_selected "$name"; then
            should_run=true
            break
        fi
    done

    if [ "$should_run" != true ]; then
        return
    fi

    print_header "Real-world External Library - ts-toolbelt"
    ensure_ts_toolbelt_fixture
    echo -e "${GREEN}✓${NC} ts-toolbelt pinned at $(git -C "$TS_TOOLBELT_DIR" rev-parse --short HEAD)"

    # Run as isolated file probes using compiler defaults.
    local lib_args=""

    local files
    if [ "$QUICK_MODE" = true ]; then
        files=("sources/Iteration/Iteration.ts")
    else
        files=(
            "sources/Iteration/Iteration.ts"
            "sources/Misc/BuiltIn.ts"
            "sources/Object/Invert.ts"
            "sources/Any/Compute.ts"
        )
    fi

    local rel
    for rel in "${files[@]}"; do
        local full_path="$TS_TOOLBELT_DIR/$rel"
        if [ -f "$full_path" ]; then
            run_benchmark "ts-toolbelt/${rel#sources/}" "$full_path" "$lib_args"
            echo
        fi
    done
}

run_ts_essentials_benchmarks() {
    local benchmark_names=(
        "ts-essentials/xor.ts"
        "ts-essentials/paths.ts"
        "ts-essentials/deep-pick.ts"
        "ts-essentials/deep-readonly.ts"
    )

    local should_run=false
    local name
    for name in "${benchmark_names[@]}"; do
        if is_benchmark_selected "$name"; then
            should_run=true
            break
        fi
    done

    if [ "$should_run" != true ]; then
        return
    fi

    print_header "Real-world External Library - ts-essentials"
    ensure_ts_essentials_fixture
    echo -e "${GREEN}✓${NC} ts-essentials pinned at $(git -C "$TS_ESSENTIALS_DIR" rev-parse --short HEAD)"

    # Run as isolated file probes using compiler defaults.
    local lib_args=""

    local files
    if [ "$QUICK_MODE" = true ]; then
        files=("lib/paths/index.ts")
    else
        files=(
            "lib/xor/index.ts"
            "lib/paths/index.ts"
            "lib/deep-pick/index.ts"
            "lib/deep-readonly/index.ts"
        )
    fi

    local rel
    for rel in "${files[@]}"; do
        local full_path="$TS_ESSENTIALS_DIR/$rel"
        if [ -f "$full_path" ]; then
            local label
            label="$(echo "${rel#lib/}" | sed 's#/index.ts$#.ts#')"
            run_benchmark "ts-essentials/$label" "$full_path" "$lib_args"
            echo
        fi
    done
}

run_nextjs_benchmarks() {
    if [ "$NEXTJS_BENCHMARK_ENABLED" != "1" ]; then
        return
    fi

    if ! is_benchmark_selected "nextjs"; then
        return
    fi

    print_header "Real-world External Project - next.js (full project)"
    ensure_nextjs_fixture
    echo -e "${GREEN}✓${NC} next.js pinned at $(git -C "$NEXTJS_DIR" rev-parse --short HEAD)"

    local tsconfig="$NEXTJS_DIR/packages/next/tsconfig.json"
    local src_dir="$NEXTJS_DIR/packages/next/src"

    if [ ! -f "$tsconfig" ]; then
        echo -e "${RED}✗ tsconfig not found: $tsconfig${NC}"
        return
    fi

    run_project_benchmark "nextjs" "$tsconfig" "$src_dir"
    echo
}

generate_synthetic_file() {
    local class_count="$1"
    local output="$2"
    
    cat > "$output" << 'HEADER'
// Synthetic TypeScript benchmark file
// Auto-generated for performance testing

HEADER

    for ((i=0; i<class_count; i++)); do
        cat >> "$output" << EOF
export interface Config$i {
    readonly id: number;
    name: string;
    enabled: boolean;
    options?: Record<string, unknown>;
}

export class Service$i implements Config$i {
    readonly id: number = $i;
    name: string;
    enabled: boolean = true;
    private items: string[] = [];

    constructor(name: string) {
        this.name = name;
    }

    getId(): number {
        return this.id;
    }

    getName(): string {
        return this.name;
    }

    setName(value: string): void {
        this.name = value;
    }

    isEnabled(): boolean {
        return this.enabled;
    }

    addItem(item: string): void {
        this.items.push(item);
    }

    getItems(): readonly string[] {
        return this.items;
    }

    static create(name: string): Service$i {
        return new Service$i(name);
    }
}

EOF
    done
}

generate_complex_file() {
    local func_count="$1"
    local output="$2"
    
    cat > "$output" << 'HEADER'
// Complex TypeScript with generics, unions, and conditional types
/// <reference lib="es2015.promise" />

type DeepPartial<T> = T extends object ? { [P in keyof T]?: DeepPartial<T[P]> } : T;

interface Result<T, E = Error> {
    ok: boolean;
    value?: T;
    error?: E;
}

HEADER

    for ((i=0; i<func_count; i++)); do
        cat >> "$output" << EOF
async function process$i<T extends Record<string, unknown>>(
    input: T,
    options?: DeepPartial<{ timeout: number; retries: number }>
): Promise<Result<T>> {
    const timeout = options?.timeout ?? 1000;
    const retries = options?.retries ?? 3;
    
    for (let attempt = 0; attempt < retries; attempt++) {
        try {
            const result = await Promise.resolve(input);
            if (timeout < 0) {
                throw new Error('timeout');
            }
            return { ok: true, value: result };
        } catch (e) {
            if (attempt === retries - 1) {
                return { ok: false, error: e as Error };
            }
        }
    }
    return { ok: false, error: new Error('exhausted') };
}

EOF
    done
}

generate_deeppartial_optional_chain_file() {
    local func_count="$1"
    local output="$2"

    cat > "$output" << 'HEADER'
// DeepPartial + optional-chain hotspot benchmark.
// This isolates recursive mapped-type expansion on repeated property access.
/// <reference lib="es2015.promise" />

type DeepPartial<T> = T extends object ? { [P in keyof T]?: DeepPartial<T[P]> } : T;
type Normalize<T> = T extends object ? { [P in keyof T]: Normalize<T[P]> } : T;
type DeepInput<T> = DeepPartial<Normalize<T>>;

interface RetryOptions {
    timeout: number;
    retries: number;
    nested: {
        transport: {
            backoff: {
                base: number;
                max: number;
                jitter: number;
            };
        };
        flags: {
            fast: boolean;
            safe: boolean;
        };
    };
}

interface Result<T, E = Error> {
    ok: boolean;
    value?: T;
    error?: E;
}

HEADER

    for ((i=0; i<func_count; i++)); do
        cat >> "$output" << EOF
async function deepPartialHotspot$i<T extends Record<string, unknown>>(
    input: T,
    options?: DeepInput<RetryOptions>
): Promise<Result<T>> {
    const timeout = options?.timeout ?? 1000;
    const base = options?.nested?.transport?.backoff?.base ?? 10;
    const max = options?.nested?.transport?.backoff?.max ?? 100;
    const jitter = options?.nested?.transport?.backoff?.jitter ?? 1;
    const safe = options?.nested?.flags?.safe ?? true;
    const fast = options?.nested?.flags?.fast ?? false;
    const retries = options?.retries ?? (safe ? 3 : 1);

    for (let attempt = 0; attempt < retries; attempt++) {
        try {
            const result = await Promise.resolve(input);
            const budget = timeout + base + max + jitter + (fast ? 1 : 0);
            if (budget < 0) {
                throw new Error('timeout');
            }
            return { ok: true, value: result };
        } catch (e) {
            if (attempt === retries - 1) {
                return { ok: false, error: e as Error };
            }
        }
    }
    return { ok: false, error: new Error('exhausted') };
}

EOF
    done
}

generate_shallow_optional_chain_file() {
    local func_count="$1"
    local output="$2"

    cat > "$output" << 'HEADER'
// Shallow optional-chain control benchmark.
// Same structure as DeepPartial hotspot but without recursive mapped types.
/// <reference lib="es2015.promise" />

interface RetryOptionsShallow {
    timeout?: number;
    retries?: number;
    nested?: {
        transport?: {
            backoff?: {
                base?: number;
                max?: number;
                jitter?: number;
            };
        };
        flags?: {
            fast?: boolean;
            safe?: boolean;
        };
    };
}

interface Result<T, E = Error> {
    ok: boolean;
    value?: T;
    error?: E;
}

HEADER

    for ((i=0; i<func_count; i++)); do
        cat >> "$output" << EOF
async function shallowOptionalControl$i<T extends Record<string, unknown>>(
    input: T,
    options?: RetryOptionsShallow
): Promise<Result<T>> {
    const timeout = options?.timeout ?? 1000;
    const base = options?.nested?.transport?.backoff?.base ?? 10;
    const max = options?.nested?.transport?.backoff?.max ?? 100;
    const jitter = options?.nested?.transport?.backoff?.jitter ?? 1;
    const safe = options?.nested?.flags?.safe ?? true;
    const fast = options?.nested?.flags?.fast ?? false;
    const retries = options?.retries ?? (safe ? 3 : 1);

    for (let attempt = 0; attempt < retries; attempt++) {
        try {
            const result = await Promise.resolve(input);
            const budget = timeout + base + max + jitter + (fast ? 1 : 0);
            if (budget < 0) {
                throw new Error('timeout');
            }
            return { ok: true, value: result };
        } catch (e) {
            if (attempt === retries - 1) {
                return { ok: false, error: e as Error };
            }
        }
    }
    return { ok: false, error: new Error('exhausted') };
}

EOF
    done
}

generate_typed_arrays_file() {
    local output="$1"

    cat > "$output" << 'HEADER'
// Typed array benchmark fixture used by bench-vs-tsgo.sh.
// Keep this strict/explicit so all compilers can parse and type-check it.

function createTypedArrayInstancesFromLength(length: number) {
    const typedArrays = [];
    typedArrays[0] = new Int8Array(length);
    typedArrays[1] = new Uint8Array(length);
    typedArrays[2] = new Int16Array(length);
    typedArrays[3] = new Uint16Array(length);
    typedArrays[4] = new Int32Array(length);
    typedArrays[5] = new Uint32Array(length);
    typedArrays[6] = new Float32Array(length);
    typedArrays[7] = new Float64Array(length);
    typedArrays[8] = new Uint8ClampedArray(length);
    return typedArrays;
}

function createTypedArrayInstancesFromArrayLike(obj: ArrayLike<number>) {
    const typedArrays = [];
    typedArrays[0] = new Int8Array(obj);
    typedArrays[1] = new Uint8Array(obj);
    typedArrays[2] = new Int16Array(obj);
    typedArrays[3] = new Uint16Array(obj);
    typedArrays[4] = new Int32Array(obj);
    typedArrays[5] = new Uint32Array(obj);
    typedArrays[6] = new Float32Array(obj);
    typedArrays[7] = new Float64Array(obj);
    typedArrays[8] = new Uint8ClampedArray(obj);
    return typedArrays;
}

function createTypedArraysFromMapFn(
    obj: ArrayLike<number>,
    mapFn: (n: number, v: number) => number
) {
    const typedArrays = [];
    typedArrays[0] = Int8Array.from(obj, mapFn);
    typedArrays[1] = Uint8Array.from(obj, mapFn);
    typedArrays[2] = Int16Array.from(obj, mapFn);
    typedArrays[3] = Uint16Array.from(obj, mapFn);
    typedArrays[4] = Int32Array.from(obj, mapFn);
    typedArrays[5] = Uint32Array.from(obj, mapFn);
    typedArrays[6] = Float32Array.from(obj, mapFn);
    typedArrays[7] = Float64Array.from(obj, mapFn);
    typedArrays[8] = Uint8ClampedArray.from(obj, mapFn);
    return typedArrays;
}

const values: number[] = [1, 2, 3, 4];
const mapped = createTypedArraysFromMapFn(values, (n, i) => n + i);
const fromLength = createTypedArrayInstancesFromLength(128);
const fromArrayLike = createTypedArrayInstancesFromArrayLike(values);
const sampleCount = mapped.length + fromLength.length + fromArrayLike.length;
HEADER
}

generate_union_file() {
    local member_count="$1"
    local output="$2"
    
    cat > "$output" << 'HEADER'
// Union type stress test - discriminated unions with many members

HEADER

    # Generate union type
    echo "type StressEvent =" >> "$output"
    for ((i=0; i<member_count; i++)); do
        if [ $i -eq $((member_count - 1)) ]; then
            echo "    | { type: 'event$i'; payload$i: string; timestamp: number };" >> "$output"
        else
            echo "    | { type: 'event$i'; payload$i: string; timestamp: number }" >> "$output"
        fi
    done
    
    echo "" >> "$output"
    
    # Generate handler function with exhaustive switch
    cat >> "$output" << 'HANDLER_START'
function handleEvent(event: StressEvent): string {
    switch (event.type) {
HANDLER_START

    for ((i=0; i<member_count; i++)); do
        echo "        case 'event$i': return event.payload$i;" >> "$output"
    done
    
    cat >> "$output" << 'HANDLER_END'
        default:
            throw new Error('unreachable');
    }
}

HANDLER_END

    # Generate some type narrowing tests
    for ((i=0; i<member_count; i+=10)); do
        cat >> "$output" << EOF
function isEvent$i(e: StressEvent): e is Extract<StressEvent, { type: 'event$i' }> {
    return e.type === 'event$i';
}

EOF
    done
}

# =============================================================================
# SOLVER STRESS TEST GENERATORS
# =============================================================================
# These generators create files that stress specific solver limits defined in
# src/limits.rs. They push close to (but under) hard limits to find perf cliffs.

# Stress: MAX_INSTANTIATION_DEPTH (50), MAX_SUBTYPE_DEPTH (100)
generate_recursive_generic_file() {
    local depth="$1"
    local output="$2"
    
    cat > "$output" << 'HEADER'
// Recursive generic type instantiation stress test
// Pushes MAX_INSTANTIATION_DEPTH and subtype checking limits

type LinkedList<T> = { value: T; next: LinkedList<T> | null };
type DeepPartial<T> = T extends object ? { [P in keyof T]?: DeepPartial<T[P]> } : T;
type DeepReadonly<T> = T extends object ? { readonly [P in keyof T]: DeepReadonly<T[P]> } : T;

HEADER

    # Generate recursive wrapper types
    for ((i=0; i<depth; i++)); do
        echo "type Wrap$i<T> = { layer$i: T };" >> "$output"
    done
    
    # Generate deeply nested instantiation
    echo "" >> "$output"
    echo "// Deep instantiation chain" >> "$output"
    local chain="string"
    local max_chain=$((depth < 40 ? depth : 40))
    for ((i=max_chain-1; i>=0; i--)); do
        chain="Wrap$i<$chain>"
    done
    echo "type DeepWrapped = $chain;" >> "$output"
    
    # Force evaluation with assignments
    echo "" >> "$output"
    echo "declare const deep: DeepWrapped;" >> "$output"
    echo "declare function extract<T>(x: Wrap0<T>): T;" >> "$output"
    echo "const _test = extract(deep);" >> "$output"
    
    # Add recursive type checks
    echo "" >> "$output"
    echo "// Recursive list operations" >> "$output"
    echo "declare const list: LinkedList<number>;" >> "$output"
    echo "declare function mapList<T, U>(l: LinkedList<T>, f: (x: T) => U): LinkedList<U>;" >> "$output"
    echo "const mapped = mapList(list, x => x.toString());" >> "$output"
}

# Stress: MAX_DISTRIBUTION_SIZE (100), MAX_EVALUATE_DEPTH (50)
generate_conditional_distribution_file() {
    local member_count="$1"
    local output="$2"
    
    cat > "$output" << 'HEADER'
// Conditional type distribution stress test
// Tests large union distribution in conditional types

type ExtractString<T> = T extends string ? T : never;
type ExtractNumber<T> = T extends number ? T : never;
type ExtractArrayType<T> = T extends (infer U)[] ? U : never;
type ToArray<T> = T extends any ? T[] : never;
type Flatten<T> = T extends (infer U)[] ? Flatten<U> : T;

HEADER

    # Generate a large union type
    echo "type BigUnion =" >> "$output"
    for ((i=0; i<member_count; i++)); do
        if [ $i -eq $((member_count - 1)) ]; then
            echo "    | 'value$i';" >> "$output"
        else
            echo "    | 'value$i'" >> "$output"
        fi
    done
    
    # Apply conditional types that distribute over the union
    echo "" >> "$output"
    echo "// Distributive conditional type applications" >> "$output"
    echo "type Distributed1 = ToArray<BigUnion>;" >> "$output"
    echo "type Distributed2 = ExtractString<BigUnion | number>;" >> "$output"
    
    # Chain multiple conditional transformations
    cat >> "$output" << 'EOF'

type ChainedConditional<T> =
    T extends string ? `prefix_${T}` :
    T extends number ? T :
    T extends boolean ? (T extends true ? 1 : 0) :
    never;

type Applied = ChainedConditional<BigUnion>;

// Nested conditional
type NestedConditional<T> =
    T extends `value${infer N}` ? N extends `${infer D}${infer Rest}` ? D : never : never;

type Extracted = NestedConditional<BigUnion>;

EOF

    # Force type evaluation with declarations
    echo "declare const distributed: Distributed1;" >> "$output"
    echo "declare const applied: Applied;" >> "$output"
    echo "declare const extracted: Extracted;" >> "$output"
}

# Stress: MAX_MAPPED_KEYS (500)
generate_mapped_type_file() {
    local key_count="$1"
    local output="$2"
    
    cat > "$output" << 'HEADER'
// Mapped type expansion stress test
// Tests MAX_MAPPED_KEYS limit and mapped type evaluation

type MyOptional<T> = { [K in keyof T]?: T[K] };
type MyRequired<T> = { [K in keyof T]-?: T[K] };
type MyReadonly<T> = { readonly [K in keyof T]: T[K] };
type MyMutable<T> = { -readonly [K in keyof T]: T[K] };

// Advanced mapped types
type Getters<T> = { [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K] };
type Setters<T> = { [K in keyof T as `set${Capitalize<string & K>}`]: (val: T[K]) => void };

HEADER

    # Generate a type with many properties
    echo "interface BigObject {" >> "$output"
    for ((i=0; i<key_count; i++)); do
        echo "    prop$i: string;" >> "$output"
    done
    echo "}" >> "$output"
    
    # Apply various mapped type transformations
    echo "" >> "$output"
    echo "// Mapped type transformations" >> "$output"
    echo "type Partial1 = MyOptional<BigObject>;" >> "$output"
    echo "type Readonly1 = MyReadonly<BigObject>;" >> "$output"
    echo "type Both = MyReadonly<MyOptional<BigObject>>;" >> "$output"
    echo "" >> "$output"
    echo "type BigGetters = Getters<BigObject>;" >> "$output"
    echo "type BigSetters = Setters<BigObject>;" >> "$output"
    
    # Nested mapped type
    cat >> "$output" << 'EOF'

// Nested mapped type
type DeepOptional<T> = T extends object ? { [K in keyof T]?: DeepOptional<T[K]> } : T;
type DeepBigObject = DeepOptional<BigObject>;

EOF

    # Force evaluation
    echo "declare const partial: Partial1;" >> "$output"
    echo "declare const getters: BigGetters;" >> "$output"
    echo "declare const deep: DeepBigObject;" >> "$output"
    echo "const _prop0 = partial.prop0;" >> "$output"
}

# Stress: TEMPLATE_LITERAL_EXPANSION_LIMIT (100,000)
generate_template_literal_file() {
    local variant_count="$1"
    local output="$2"
    
    cat > "$output" << 'HEADER'
// Template literal type expansion stress test
// Tests Cartesian product explosion prevention

HEADER

    # Generate multiple union types for Cartesian product
    local max_variants=$((variant_count < 50 ? variant_count : 50))
    
    echo "type Colors =" >> "$output"
    for ((i=0; i<max_variants; i++)); do
        if [ $i -eq $((max_variants - 1)) ]; then
            echo "    | 'color$i';" >> "$output"
        else
            echo "    | 'color$i'" >> "$output"
        fi
    done
    
    echo "" >> "$output"
    echo "type Sizes =" >> "$output"
    for ((i=0; i<max_variants; i++)); do
        if [ $i -eq $((max_variants - 1)) ]; then
            echo "    | 'size$i';" >> "$output"
        else
            echo "    | 'size$i'" >> "$output"
        fi
    done
    
    echo "" >> "$output"
    echo "type Variants =" >> "$output"
    for ((i=0; i<max_variants; i++)); do
        if [ $i -eq $((max_variants - 1)) ]; then
            echo "    | 'variant$i';" >> "$output"
        else
            echo "    | 'variant$i'" >> "$output"
        fi
    done
    
    # Template literal combining unions (Cartesian product)
    cat >> "$output" << 'EOF'

// Template literal Cartesian products
type ProductSmall = `${Colors}-${Sizes}`;
type ProductMedium = `${Colors}-${Sizes}-${Variants}`;

// String manipulation types
type Prefixed = `prefix_${Colors}`;
type Suffixed = `${Colors}_suffix`;
type Wrapped = `[${Colors}]`;

// Nested template
type NestedTemplate = `start_${`mid_${Colors}`}_end`;

EOF

    # Force evaluation
    echo "declare const product: ProductSmall;" >> "$output"
    echo "declare const prefixed: Prefixed;" >> "$output"
}

# Stress: MAX_SUBTYPE_DEPTH (100), coinductive cycle detection
generate_deep_subtype_file() {
    local depth="$1"
    local output="$2"
    
    cat > "$output" << 'HEADER'
// Deep subtype checking stress test
// Tests recursive type comparison and cycle detection

// Self-referential types
interface TreeNode<T> {
    value: T;
    children: TreeNode<T>[];
}

interface MutualA<T> {
    data: T;
    ref: MutualB<T>;
}

interface MutualB<T> {
    info: T;
    back: MutualA<T>;
}

// Recursive JSON type
type Json = string | number | boolean | null | Json[] | { [key: string]: Json };

HEADER

    # Generate deep class hierarchy for variance checking
    echo "// Deep class hierarchy for subtype checking" >> "$output"
    echo "class Base0 { x0: string = ''; }" >> "$output"
    local max_depth=$((depth < 50 ? depth : 50))
    for ((i=1; i<max_depth; i++)); do
        local prev=$((i - 1))
        echo "class Base$i extends Base$prev { x$i: string = ''; }" >> "$output"
    done
    
    # Generate covariant/contravariant positions
    cat >> "$output" << 'EOF'

// Variance stress with function types
type CovariantContainer<T> = { get(): T };
type ContravariantContainer<T> = { set(x: T): void };
type InvariantContainer<T> = { get(): T; set(x: T): void };

// Bivariant method position
interface BivariantMethods<T> {
    method(x: T): T;
}

EOF

    # Deep nested function type
    local deepfn="string"
    local max_fn_depth=$((depth < 30 ? depth : 30))
    for ((i=0; i<max_fn_depth; i++)); do
        deepfn="(x: $deepfn) => void"
    done
    echo "" >> "$output"
    echo "type DeepFunction = $deepfn;" >> "$output"
    
    # Force subtype checks
    cat >> "$output" << 'EOF'

// Force subtype checks
declare const tree1: TreeNode<string>;
declare const tree2: TreeNode<string | number>;
const _check: TreeNode<string | number> = tree1;

declare const mutual: MutualA<string>;
declare function acceptMutual(x: MutualA<string | number>): void;
acceptMutual(mutual);

// JSON type checks
declare const json1: Json;
declare const json2: { nested: Json };
const _jsonCheck: Json = json2;

EOF
}

# Stress: Intersection normalization and property merging
generate_intersection_file() {
    local count="$1"
    local output="$2"
    
    cat > "$output" << 'HEADER'
// Intersection type stress test
// Tests intersection normalization and property merging

HEADER

    # Generate many interfaces to intersect
    for ((i=0; i<count; i++)); do
        echo "interface Part$i {" >> "$output"
        echo "    prop$i: string;" >> "$output"
        echo "    shared: number;" >> "$output"
        echo "    method$i(): number;" >> "$output"
        echo "}" >> "$output"
        echo "" >> "$output"
    done
    
    # Create large intersections
    local intersection="Part0"
    local max_intersect=$((count < 50 ? count : 50))
    for ((i=1; i<max_intersect; i++)); do
        intersection="$intersection & Part$i"
    done
    echo "type BigIntersection = $intersection;" >> "$output"
    
    # Function overload intersection
    cat >> "$output" << 'EOF'

// Function overload intersection
type OverloadIntersection = 
    ((x: string) => string) &
    ((x: number) => number) &
    ((x: boolean) => boolean);

// Generic intersection
type GenericIntersection<T, U> = T & U;

EOF

    # Force evaluation
    echo "" >> "$output"
    echo "declare const big: BigIntersection;" >> "$output"
    echo "const _prop0 = big.prop0;" >> "$output"
    echo "const _shared = big.shared;" >> "$output"
    local last=$((count - 1))
    if [ $last -lt 50 ]; then
        echo "const _propLast = big.prop$last;" >> "$output"
    fi
}

# Stress: Inference variable instantiation in conditional types
generate_infer_stress_file() {
    local count="$1"
    local output="$2"
    
    cat > "$output" << 'HEADER'
// Infer keyword stress test
// Tests inference variable resolution in conditional types

type UnwrapPromise<T> = T extends Promise<infer U> ? U : T;
type UnwrapArray<T> = T extends (infer U)[] ? U : T;
type MyParameters<T> = T extends (...args: infer P) => any ? P : never;
type MyReturnType<T> = T extends (...args: any[]) => infer R ? R : never;

// Multi-infer conditional
type FirstAndRest<T> = T extends [infer First, ...infer Rest] ? { first: First; rest: Rest } : never;

// Nested infer
type DeepUnwrap<T> = 
    T extends Promise<infer U> ? DeepUnwrap<U> :
    T extends (infer V)[] ? DeepUnwrap<V>[] :
    T;

// Infer in template literal
type ExtractPrefix<T> = T extends `${infer P}_${string}` ? P : never;

// Infer with constraints
type ExtractIfString<T> = T extends infer U extends string ? U : never;

HEADER

    # Generate functions with many parameters to test Parameters<T>
    local max_funcs=$((count < 30 ? count : 30))
    for ((i=0; i<max_funcs; i++)); do
        echo "declare function func$i(" >> "$output"
        for ((j=0; j<=i; j++)); do
            if [ $j -eq $i ]; then
                echo "    arg$j: string" >> "$output"
            else
                echo "    arg$j: string," >> "$output"
            fi
        done
        echo "): number;" >> "$output"
        echo "" >> "$output"
        echo "type Params$i = MyParameters<typeof func$i>;" >> "$output"
        echo "type Return$i = MyReturnType<typeof func$i>;" >> "$output"
        echo "" >> "$output"
    done
    
    # Force evaluation with complex nested inference
    cat >> "$output" << 'EOF'

// Complex nested inference
type ComplexInfer<T> = T extends { 
    data: infer D; 
    nested: { value: infer V }[] 
} ? { data: D; values: V[] } : never;

interface TestData {
    data: string;
    nested: { value: number }[];
}

type Inferred = ComplexInfer<TestData>;

EOF

    echo "declare const params: Params$((max_funcs - 1));" >> "$output"
    echo "declare const inferred: Inferred;" >> "$output"
}

# Stress: Control flow analysis with many branches
generate_cfa_stress_file() {
    local branch_count="$1"
    local output="$2"
    
    cat > "$output" << 'HEADER'
// Control flow analysis stress test
// Tests type narrowing with many branches

type Status = 'pending' | 'active' | 'completed' | 'failed' | 'cancelled';

interface BaseEntity {
    id: string;
    status: Status;
}

HEADER

    # Generate discriminated union
    echo "type Entity =" >> "$output"
    for ((i=0; i<branch_count; i++)); do
        if [ $i -eq $((branch_count - 1)) ]; then
            echo "    | { kind: 'type$i'; data$i: string; common: number };" >> "$output"
        else
            echo "    | { kind: 'type$i'; data$i: string; common: number }" >> "$output"
        fi
    done
    
    # Generate exhaustive switch
    cat >> "$output" << 'EOF'

function processEntity(e: Entity): string {
    switch (e.kind) {
EOF

    for ((i=0; i<branch_count; i++)); do
        echo "        case 'type$i': return e.data$i;" >> "$output"
    done
    
    cat >> "$output" << 'EOF'
        default:
            throw new Error('unreachable');
    }
}

EOF

    # Generate many branch checks without relying on final-else narrowing.
    echo "function processWithIf(e: Entity): string {" >> "$output"
    for ((i=0; i<branch_count; i++)); do
        echo "    if (e.kind === 'type$i') {" >> "$output"
        echo "        return e.data$i;" >> "$output"
        echo "    }" >> "$output"
    done
    echo "    return processEntity(e);" >> "$output"
    echo "}" >> "$output"
    
    # Type guard functions
    echo "" >> "$output"
    for ((i=0; i<branch_count; i+=5)); do
        cat >> "$output" << EOF
function isType$i(e: Entity): e is Extract<Entity, { kind: 'type$i' }> {
    return e.kind === 'type$i';
}

EOF
    done
}

# =============================================================================
# O(N²) ALGORITHMIC PATTERN BENCHMARKS
# =============================================================================
# These generators create files that specifically stress the three known O(N²)
# algorithmic patterns in the solver that Salsa memoization alone cannot fix.
# See docs/todo/05_algorithmic_fixes.md for details.

# Stress: Best Common Type — O(N²) in infer.rs:1060
# N candidates × N subtype checks per candidate.
# Triggered when many return statements / array elements need a common type.
generate_bct_stress_file() {
    local count="$1"
    local output="$2"

    cat > "$output" << 'HEADER'
// Best Common Type (BCT) O(N²) stress test
// Targets: infer.rs best_common_type() — N candidates × N subtype checks
//
// Each class in the hierarchy is a distinct type candidate. When the compiler
// infers the type of an array literal or multi-return function, it must find
// the "best common type" by checking every candidate against every other.

HEADER

    # Build a class hierarchy so types are related but distinct
    echo "class Base { base: string = ''; }" >> "$output"
    for ((i=0; i<count; i++)); do
        echo "class Derived$i extends Base { prop$i: number = $i; }" >> "$output"
    done
    echo "" >> "$output"

    # 1. Array literal with N distinct derived types — triggers BCT
    echo "// Array literal: BCT must find common type among $count candidates" >> "$output"
    echo -n "const items = [" >> "$output"
    for ((i=0; i<count; i++)); do
        if [ $i -gt 0 ]; then echo -n ", " >> "$output"; fi
        echo -n "new Derived$i()" >> "$output"
    done
    echo "];" >> "$output"
    echo "" >> "$output"

    # 2. Function with N return statements — triggers BCT on return type
    echo "// Function with $count return branches — BCT on return type inference" >> "$output"
    echo "function pickOne(index: number) {" >> "$output"
    for ((i=0; i<count; i++)); do
        echo "    if (index === $i) return new Derived$i();" >> "$output"
    done
    echo "    return new Base();" >> "$output"
    echo "}" >> "$output"
    echo "" >> "$output"

    # 3. Generic function called with N different argument types
    # This accumulates inference candidates that go through BCT
    echo "// Generic calls accumulating $count candidates" >> "$output"
    echo "function identity<T>(x: T): T { return x; }" >> "$output"
    echo -n "const mixed = [" >> "$output"
    for ((i=0; i<count; i++)); do
        if [ $i -gt 0 ]; then echo -n ", " >> "$output"; fi
        echo -n "identity(new Derived$i())" >> "$output"
    done
    echo "];" >> "$output"
    echo "" >> "$output"

    # 4. Conditional expression chains — each branch is a BCT candidate
    echo "// Ternary chain: $count candidates for common type" >> "$output"
    echo -n "declare const flag: number;" >> "$output"
    echo "" >> "$output"
    echo -n "const chosen = " >> "$output"
    for ((i=0; i<count; i++)); do
        echo -n "flag === $i ? new Derived$i() : " >> "$output"
    done
    echo "new Base();" >> "$output"

    # Force type usage
    echo "" >> "$output"
    echo "const _base: Base = items[0];" >> "$output"
    echo "const _picked: Base = pickOne(0);" >> "$output"
    echo "const _chosen: Base = chosen;" >> "$output"
}

# Stress: Constraint Conflict Detection — O(N²) in infer.rs:135
# N² upper bound pairs + M×N lower×upper bound cross-checks.
# Triggered when a type parameter accumulates many bounds through usage.
generate_constraint_conflict_file() {
    local count="$1"
    local output="$2"

    cat > "$output" << 'HEADER'
// Constraint Conflict Detection O(N²) stress test
// Targets: infer.rs detect_conflicts() — N² upper bound pairs + M×N lower×upper
//
// When a generic type parameter is used in many positions, the solver collects
// lower bounds (argument types) and upper bounds (extends constraints, parameter
// positions). Conflict detection checks all pairs for compatibility.

HEADER

    # Generate many interfaces that will become upper bounds
    for ((i=0; i<count; i++)); do
        echo "interface Constraint$i { key$i: string; shared: number; }" >> "$output"
    done
    echo "" >> "$output"

    # Function where T is constrained by many extends clauses via overloads/conditionals
    # Each call site adds bounds to T's constraint set
    echo "// Function with type parameter accumulating bounds from $count call sites" >> "$output"
    for ((i=0; i<count; i++)); do
        echo "declare function constrain$i<T extends Constraint$i>(x: T): T;" >> "$output"
    done
    echo "" >> "$output"

    # Create objects satisfying various combinations of constraints
    echo "// Objects that satisfy multiple constraints" >> "$output"
    for ((i=0; i<count; i++)); do
        echo -n "const obj$i = { shared: $i" >> "$output"
        # Each object satisfies constraints 0..i
        for ((j=0; j<=i && j<count; j++)); do
            echo -n ", key$j: 'val'" >> "$output"
        done
        echo " };" >> "$output"
    done
    echo "" >> "$output"

    # Call constrain functions — each call adds lower + upper bounds
    echo "// Each call adds lower bounds (arg type) and upper bounds (extends Constraint$i)" >> "$output"
    for ((i=0; i<count; i++)); do
        echo "const res$i = constrain$i(obj$i);" >> "$output"
    done
    echo "" >> "$output"

    # Generic function that collects many bounds on a single type parameter
    echo "// Single type param T accumulating $count bounds" >> "$output"
    echo -n "function multiConstrained<T extends " >> "$output"
    for ((i=0; i<count; i++)); do
        if [ $i -gt 0 ]; then echo -n " & " >> "$output"; fi
        echo -n "Constraint$i" >> "$output"
    done
    echo ">(x: T): T { return x; }" >> "$output"
    echo "" >> "$output"

    # Build an object satisfying all constraints — forces full conflict check
    echo -n "const allConstraints = { shared: 0" >> "$output"
    for ((i=0; i<count; i++)); do
        echo -n ", key$i: 'val'" >> "$output"
    done
    echo " };" >> "$output"
    echo "const _result = multiConstrained(allConstraints);" >> "$output"
}

# Stress: Mapped Type Expansion with Complex Templates — O(N × template_size)
# in evaluate_rules/mapped.rs:157
# N properties × instantiate+evaluate per property, with non-trivial templates.
# The existing generate_mapped_type_file uses simple templates (T[K]).
# This version uses complex conditional templates that are expensive to evaluate.
generate_mapped_complex_template_file() {
    local key_count="$1"
    local output="$2"

    cat > "$output" << 'HEADER'
// Mapped Type Complex Template Expansion O(N²) stress test
// Targets: evaluate_rules/mapped.rs — N properties × expensive template evaluation
//
// Unlike simple homomorphic mapped types ({ [K in keyof T]: T[K] }) where the
// template is trivial, these use conditional types and nested mapped types in
// the template position, making each property evaluation expensive.

// Utility types with non-trivial evaluation
type DeepPartial<T> = T extends object ? { [P in keyof T]?: DeepPartial<T[P]> } : T;
type Stringify<T> = { [K in keyof T]: T[K] extends number ? string : T[K] extends boolean ? 'true' | 'false' : T[K] extends string ? T[K] : string };
type Validate<T> = { [K in keyof T]: T[K] extends string ? { valid: true; value: T[K] } : T[K] extends number ? { valid: true; value: T[K] } : { valid: false; value: never } };
type Nullable<T> = { [K in keyof T]: T[K] | null | undefined };
type Promisify<T> = { [K in keyof T]: Promise<T[K]> };

// Complex conditional template: each property evaluation triggers conditional
// type distribution and nested type instantiation
type FormField<T> =
    T extends string ? { type: 'text'; value: T; validate: (v: string) => boolean }
  : T extends number ? { type: 'number'; value: T; validate: (v: number) => boolean }
  : T extends boolean ? { type: 'checkbox'; value: T; validate: (v: boolean) => boolean }
  : T extends (infer U)[] ? { type: 'list'; items: FormField<U>[]; validate: (v: U[]) => boolean }
  : T extends object ? { type: 'group'; fields: FormFields<T>; validate: (v: T) => boolean }
  : { type: 'unknown'; value: T };

type FormFields<T> = { [K in keyof T]: FormField<T[K]> };

HEADER

    # Generate a large interface with mixed property types
    echo "interface BigModel {" >> "$output"
    for ((i=0; i<key_count; i++)); do
        local mod=$((i % 5))
        case $mod in
            0) echo "    field$i: string;" >> "$output" ;;
            1) echo "    field$i: number;" >> "$output" ;;
            2) echo "    field$i: boolean;" >> "$output" ;;
            3) echo "    field$i: string[];" >> "$output" ;;
            4) echo "    field$i: { nested: string; count: number };" >> "$output" ;;
        esac
    done
    echo "}" >> "$output"
    echo "" >> "$output"

    # Apply complex mapped types — each triggers per-property conditional evaluation
    echo "// Each mapped type application evaluates a conditional template for $key_count properties" >> "$output"
    echo "type BigForm = FormFields<BigModel>;" >> "$output"
    echo "type BigStringified = Stringify<BigModel>;" >> "$output"
    echo "type BigValidated = Validate<BigModel>;" >> "$output"
    echo "type BigNullable = Nullable<BigModel>;" >> "$output"
    echo "type BigPromises = Promisify<BigModel>;" >> "$output"
    echo "type BigDeepPartial = DeepPartial<BigModel>;" >> "$output"
    echo "" >> "$output"

    # Chained mapped types — composition multiplies the per-property cost
    echo "// Chained: each composition re-evaluates all $key_count properties" >> "$output"
    echo "type Chained1 = Nullable<Stringify<BigModel>>;" >> "$output"
    echo "type Chained2 = Validate<Nullable<BigModel>>;" >> "$output"
    echo "type Chained3 = FormFields<Nullable<BigModel>>;" >> "$output"
    echo "" >> "$output"

    # Force evaluation with declarations
    echo "declare const form: BigForm;" >> "$output"
    echo "declare const stringified: BigStringified;" >> "$output"
    echo "declare const validated: BigValidated;" >> "$output"
    echo "declare const chained: Chained3;" >> "$output"
    echo "" >> "$output"

    # Access properties to force full expansion
    echo "const _f0 = form.field0;" >> "$output"
    echo "const _s0 = stringified.field0;" >> "$output"
    echo "const _v0 = validated.field0;" >> "$output"
    local last=$((key_count - 1))
    echo "const _fLast = form.field$last;" >> "$output"
    echo "const _cLast = chained.field$last;" >> "$output"
}

main() {
    check_prerequisites
    
    # Create temp directory for synthetic files
    TEMP_DIR=$(mktemp -d)
    trap "rm -rf $TEMP_DIR" EXIT
    
    print_header "TypeScript Compiler Test Files"
    
    # ═══════════════════════════════════════════════════════════════════════════
    # EXTRA LARGE FILES (5000+ lines) - Stress tests
    # ═══════════════════════════════════════════════════════════════════════════
    print_subheader "Extra Large Files (5000+ lines) - Stress Tests"
    
    local xl_files
    if [ "$QUICK_MODE" = true ]; then
        xl_files=(
            "TypeScript/tests/cases/compiler/manyConstExports.ts"
        )
    else
        xl_files=(
            "TypeScript/tests/cases/compiler/conditionalTypeDiscriminatingLargeUnionRegularTypeFetchingSpeedReasonable.ts"
            "TypeScript/tests/cases/compiler/manyConstExports.ts"
            "TypeScript/tests/cases/compiler/binderBinaryExpressionStress.ts"
            "TypeScript/tests/cases/compiler/binderBinaryExpressionStressJs.ts"
        )
    fi
    
    for file in "${xl_files[@]}"; do
        local full_path="$PROJECT_ROOT/$file"
        if [ -f "$full_path" ]; then
            run_benchmark "$(basename "$file")" "$full_path"
            echo
        fi
    done
    
    # ═══════════════════════════════════════════════════════════════════════════
    # LARGE FILES (1000-5000 lines) - Real-world complexity
    # ═══════════════════════════════════════════════════════════════════════════
    print_subheader "Large Files (1000-5000 lines) - Real-world Complexity"
    
    local large_files
    if [ "$QUICK_MODE" = true ]; then
        large_files=(
            "TypeScript/tests/cases/compiler/enumLiteralsSubtypeReduction.ts"
        )
    else
        large_files=(
            "TypeScript/tests/cases/compiler/enumLiteralsSubtypeReduction.ts"
            "TypeScript/tests/cases/compiler/binaryArithmeticControlFlowGraphNotTooLarge.ts"
        )
    fi
    
    for file in "${large_files[@]}"; do
        local full_path="$PROJECT_ROOT/$file"
        if [ -f "$full_path" ]; then
            run_benchmark "$(basename "$file")" "$full_path"
            echo
        fi
    done
    
    # Skip medium/small files in quick mode
    if [ "$QUICK_MODE" = true ]; then
        print_subheader "Skipping medium/small files in quick mode"
    else
        # ═══════════════════════════════════════════════════════════════════════════
        # MEDIUM FILES (200-1000 lines) - Typical modules
        # ═══════════════════════════════════════════════════════════════════════════
        print_subheader "Medium Files (200-1000 lines) - Typical Modules"
        
        local medium_files=(
            "TypeScript/tests/cases/compiler/privacyFunctionParameterDeclFile.ts"
            "TypeScript/tests/cases/compiler/privacyGloFunc.ts"
            "TypeScript/tests/cases/compiler/privacyTypeParameterOfFunctionDeclFile.ts"
        )
    
        for file in "${medium_files[@]}"; do
            local full_path="$PROJECT_ROOT/$file"
            if [ -f "$full_path" ]; then
                run_benchmark "$(basename "$file")" "$full_path"
                echo
            fi
        done
        
        # ═══════════════════════════════════════════════════════════════════════════
        # SMALL FILES (50-200 lines) - Quick iteration
        # ═══════════════════════════════════════════════════════════════════════════
        print_subheader "Small Files (50-200 lines) - Startup Overhead Test"

        local typed_arrays_file="$TEMP_DIR/typedArrays.bench.ts"
        generate_typed_arrays_file "$typed_arrays_file"
        run_benchmark "typedArrays.ts" "$typed_arrays_file"
        echo
        
        local small_files=(
            "TypeScript/tests/cases/compiler/controlFlowArrays.ts"
        )
        
        for file in "${small_files[@]}"; do
            local full_path="$PROJECT_ROOT/$file"
            if [ -f "$full_path" ]; then
                run_benchmark "$(basename "$file")" "$full_path"
                echo
            fi
        done
    fi  # End of medium/small files skip

    run_utility_types_benchmarks
    run_ts_toolbelt_benchmarks
    run_ts_essentials_benchmarks
    run_nextjs_benchmarks
    
    print_header "Synthetic Benchmarks - Scaling Test"
    
    if [ "$QUICK_MODE" = true ]; then
        print_subheader "Quick mode: reduced synthetic tests"
        
        # Just one of each type in quick mode
        local file="$TEMP_DIR/synthetic_100_classes.ts"
        generate_synthetic_file 100 "$file"
        run_benchmark "100 classes" "$file"
        echo
        
        file="$TEMP_DIR/complex_50_funcs.ts"
        generate_complex_file 50 "$file"
        run_benchmark "50 generic functions" "$file"
        echo

        file="$TEMP_DIR/deeppartial_optional_50.ts"
        generate_deeppartial_optional_chain_file 50 "$file"
        run_benchmark "DeepPartial optional-chain N=50" "$file"
        echo

        file="$TEMP_DIR/shallow_optional_50.ts"
        generate_shallow_optional_chain_file 50 "$file"
        run_benchmark "Shallow optional-chain N=50" "$file"
        echo
    else
        # Generate synthetic files of increasing size
        print_subheader "Class-heavy files (interfaces + classes)"
        
        for count in 10 50 100 200; do
            local file="$TEMP_DIR/synthetic_${count}_classes.ts"
            generate_synthetic_file "$count" "$file"
            run_benchmark "${count} classes" "$file"
            echo
        done
        
        print_subheader "Generic-heavy files (async + conditional types)"
        
        for count in 20 50 100 200; do
            local file="$TEMP_DIR/complex_${count}_funcs.ts"
            generate_complex_file "$count" "$file"
            run_benchmark "${count} generic functions" "$file"
            echo
        done

        print_subheader "DeepPartial mapped access hotspot (bottleneck probe)"

        local file="$TEMP_DIR/deeppartial_optional_400.ts"
        generate_deeppartial_optional_chain_file 400 "$file"
        run_benchmark "DeepPartial optional-chain N=400" "$file"
        echo

        file="$TEMP_DIR/shallow_optional_400.ts"
        generate_shallow_optional_chain_file 400 "$file"
        run_benchmark "Shallow optional-chain N=400" "$file"
        echo
        
        print_subheader "Union type stress test"
        
        for count in 50 100 200; do
            local file="$TEMP_DIR/union_${count}.ts"
            generate_union_file "$count" "$file"
            run_benchmark "${count} union members" "$file"
            echo
        done
    fi
    
    # ═══════════════════════════════════════════════════════════════════════════
    # SOLVER STRESS TESTS - Type system limit testing
    # ═══════════════════════════════════════════════════════════════════════════
    print_header "Solver Stress Tests - Type System Limits"
    
    if [ "$QUICK_MODE" = true ]; then
        print_subheader "Quick mode: reduced solver stress tests"
        
        # One test per category in quick mode
        local file="$TEMP_DIR/recursive_generic_25.ts"
        generate_recursive_generic_file 25 "$file"
        run_benchmark "Recursive generic depth=25" "$file"
        echo
        
        file="$TEMP_DIR/conditional_dist_50.ts"
        generate_conditional_distribution_file 50 "$file"
        run_benchmark "Conditional dist N=50" "$file"
        echo
        
        file="$TEMP_DIR/mapped_100.ts"
        generate_mapped_type_file 100 "$file"
        run_benchmark "Mapped type keys=100" "$file"
        echo
    else
        # ─────────────────────────────────────────────────────────────────────────
        print_subheader "Recursive generic instantiation (MAX_INSTANTIATION_DEPTH=50)"
        
        for depth in 20 35 45; do
            local file="$TEMP_DIR/recursive_generic_${depth}.ts"
            generate_recursive_generic_file "$depth" "$file"
            run_benchmark "Recursive generic depth=$depth" "$file"
            echo
        done
        
        # ─────────────────────────────────────────────────────────────────────────
        print_subheader "Conditional type distribution (MAX_DISTRIBUTION_SIZE=100)"
        
        for count in 50 80 95; do
            local file="$TEMP_DIR/conditional_dist_${count}.ts"
            generate_conditional_distribution_file "$count" "$file"
            run_benchmark "Conditional dist N=$count" "$file"
            echo
        done
        
        # ─────────────────────────────────────────────────────────────────────────
        print_subheader "Mapped type expansion (MAX_MAPPED_KEYS=500)"
        
        for count in 100 300 450; do
            local file="$TEMP_DIR/mapped_${count}.ts"
            generate_mapped_type_file "$count" "$file"
            run_benchmark "Mapped type keys=$count" "$file"
            echo
        done
        
        # ─────────────────────────────────────────────────────────────────────────
        print_subheader "Template literal types (TEMPLATE_LITERAL_EXPANSION_LIMIT)"
        
        for count in 20 35 45; do
            local file="$TEMP_DIR/template_${count}.ts"
            generate_template_literal_file "$count" "$file"
            run_benchmark "Template literal N=$count" "$file"
            echo
        done
        
        # ─────────────────────────────────────────────────────────────────────────
        print_subheader "Deep subtype checking (MAX_SUBTYPE_DEPTH=100)"
        
        for depth in 30 60 90; do
            local file="$TEMP_DIR/deep_subtype_${depth}.ts"
            generate_deep_subtype_file "$depth" "$file"
            run_benchmark "Deep subtype depth=$depth" "$file"
            echo
        done
        
        # ─────────────────────────────────────────────────────────────────────────
        print_subheader "Intersection types (property merging)"
        
        for count in 20 35 45; do
            local file="$TEMP_DIR/intersection_${count}.ts"
            generate_intersection_file "$count" "$file"
            run_benchmark "Intersection N=$count" "$file"
            echo
        done
        
        # ─────────────────────────────────────────────────────────────────────────
        print_subheader "Infer keyword stress (type inference)"
        
        for count in 15 25 30; do
            local file="$TEMP_DIR/infer_${count}.ts"
            generate_infer_stress_file "$count" "$file"
            run_benchmark "Infer stress N=$count" "$file"
            echo
        done
        
        # ─────────────────────────────────────────────────────────────────────────
        print_subheader "Control flow analysis (CFA with many branches)"
        
        for count in 50 100 150; do
            local file="$TEMP_DIR/cfa_${count}.ts"
            generate_cfa_stress_file "$count" "$file"
            run_benchmark "CFA branches=$count" "$file"
            echo
        done
    fi

    # ═══════════════════════════════════════════════════════════════════════════
    # O(N²) ALGORITHMIC PATTERN TESTS
    # ═══════════════════════════════════════════════════════════════════════════
    # These benchmarks target three specific O(N²) patterns in the solver that
    # Salsa memoization alone cannot fix. They serve as regression/progress
    # tracking for the algorithmic fixes described in docs/todo/05_algorithmic_fixes.md
    #
    # Pattern 1: Best Common Type (BCT) — infer.rs:1060
    #   N candidates × N subtype checks per candidate
    # Pattern 2: Constraint Conflict Detection — infer.rs:135
    #   N² upper bound pairs + M×N lower×upper cross-checks
    # Pattern 3: Mapped Type Complex Templates — evaluate_rules/mapped.rs:157
    #   N properties × expensive per-property template evaluation

    print_header "O(N²) Algorithmic Pattern Tests"

    if [ "$QUICK_MODE" = true ]; then
        print_subheader "Quick mode: reduced O(N²) pattern tests"

        local file="$TEMP_DIR/bct_50.ts"
        generate_bct_stress_file 50 "$file"
        run_benchmark "BCT candidates=50" "$file"
        echo

        file="$TEMP_DIR/constraint_conflict_30.ts"
        generate_constraint_conflict_file 30 "$file"
        run_benchmark "Constraint conflicts N=30" "$file"
        echo

        file="$TEMP_DIR/mapped_complex_50.ts"
        generate_mapped_complex_template_file 50 "$file"
        run_benchmark "Mapped complex template keys=50" "$file"
        echo
    else
        # ─────────────────────────────────────────────────────────────────────────
        print_subheader "Best Common Type — O(N²) candidate checking"

        for count in 25 50 100 200; do
            local file="$TEMP_DIR/bct_${count}.ts"
            generate_bct_stress_file "$count" "$file"
            run_benchmark "BCT candidates=$count" "$file"
            echo
        done

        # ─────────────────────────────────────────────────────────────────────────
        print_subheader "Constraint Conflict Detection — O(N²) bound pairs"

        for count in 20 50 100 200; do
            local file="$TEMP_DIR/constraint_conflict_${count}.ts"
            generate_constraint_conflict_file "$count" "$file"
            run_benchmark "Constraint conflicts N=$count" "$file"
            echo
        done

        # ─────────────────────────────────────────────────────────────────────────
        print_subheader "Mapped Type Complex Templates — O(N × template_cost)"

        for count in 25 50 100 200; do
            local file="$TEMP_DIR/mapped_complex_${count}.ts"
            generate_mapped_complex_template_file "$count" "$file"
            run_benchmark "Mapped complex template keys=$count" "$file"
            echo
        done
    fi

    if [ "$BENCHMARKS_RUN" -eq 0 ]; then
        echo -e "${RED}No benchmarks matched filter /$FILTER/.${NC}"
        echo "Try one of:"
        echo "  ./scripts/bench-vs-tsgo.sh --quick --filter 'utility-types'"
        echo "  ./scripts/bench-vs-tsgo.sh --quick --filter 'BCT|CFA'"
        return
    fi

    print_header "Results Summary"
    
    if command -v jq &>/dev/null && [ -n "$RESULTS_CSV" ]; then
        echo
        # Table header
        printf "${BOLD}%-45s %7s %6s %10s %10s %8s %8s %12s${NC}\n" \
            "Test" "Lines" "KB" "tsz(ms)" "tsgo(ms)" "Winner" "Factor" "Status"
        printf "${CYAN}%s${NC}\n" "────────────────────────────────────────────────────────────────────────────────────────────────────────────────────"
        
        # Table rows (sorted best-to-worst for tsz: tsz wins by descending factor, then tsgo wins by ascending factor)
        echo -e "$RESULTS_CSV" | awk -F',' '
            $1 != "" {
                # Create a sort key: tsz wins get +ratio, tsgo wins get -ratio, errors sink
                if ($10 != "") sort_key = -999999;
                else if ($8 == "tsz") sort_key = $9 + 0;
                else sort_key = -($9 + 0);
                print sort_key "," $0
            }
        ' | sort -t',' -k1 -rn | cut -d',' -f2- | while IFS=',' read -r name lines kb tsz_ms tsgo_ms tsz_lps tsgo_lps winner ratio status; do
            [ -z "$name" ] && continue

            # Truncate long test names
            local display_name="$name"
            if [ ${#name} -gt 44 ]; then
                display_name="${name:0:41}..."
            fi

            local status_display="${status:--}"
            local ratio_display="$ratio"
            if [ -n "$status" ]; then
                ratio_display="N/A"
                printf "%-45s %7s %6s %10s %10s ${RED}%8s${NC} ${RED}%7s${NC} ${RED}%12s${NC}\n" \
                    "$display_name" "$lines" "$kb" "$tsz_ms" "$tsgo_ms" "error" "$ratio_display" "$status_display"
            elif [ "$winner" = "tsz" ]; then
                printf "%-45s %7s %6s %10s %10s ${GREEN}%8s${NC} ${GREEN}%7sx${NC} %12s\n" \
                    "$display_name" "$lines" "$kb" "$tsz_ms" "$tsgo_ms" "$winner" "$ratio" "$status_display"
            else
                printf "%-45s %7s %6s %10s %10s ${YELLOW}%8s${NC} ${YELLOW}%7sx${NC} %12s\n" \
                    "$display_name" "$lines" "$kb" "$tsz_ms" "$tsgo_ms" "$winner" "$ratio" "$status_display"
            fi
        done
        
        # Summary line
        printf "${CYAN}%s${NC}\n" "────────────────────────────────────────────────────────────────────────────────────────────────────────────────────"
        
        # Count wins
        local tsz_wins=$(echo -e "$RESULTS_CSV" | awk -F',' '$8 == "tsz" { c++ } END { print c+0 }')
        local tsgo_wins=$(echo -e "$RESULTS_CSV" | awk -F',' '$8 == "tsgo" { c++ } END { print c+0 }')
        echo
        echo -e "${BOLD}Score:${NC} ${GREEN}tsz ${tsz_wins}${NC} vs ${YELLOW}tsgo ${tsgo_wins}${NC}"
        echo
    else
        echo
        echo -e "${YELLOW}No benchmark results recorded.${NC}"
    fi

    export_results_json
}

main "$@"
