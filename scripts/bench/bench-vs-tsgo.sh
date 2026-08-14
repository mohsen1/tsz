#!/usr/bin/env bash
#
# Benchmark: tsz vs tsgo (TypeScript 7 / typescript-go)
#
# Compares compilation performance across various file sizes and complexities.
# Requires: hyperfine (brew install hyperfine)
# tsgo is auto-installed locally (pinned) unless TSGO is explicitly provided.
#
# Usage:
#   ./scripts/bench/bench-vs-tsgo.sh                    # Full benchmark suite
#   ./scripts/bench/bench-vs-tsgo.sh --quick            # Quick smoke test (fewer runs, fewer files)
#   ./scripts/bench/bench-vs-tsgo.sh --json             # Export results to JSON
#   ./scripts/bench/bench-vs-tsgo.sh --filter 'BCT|CFA' # Run only tests matching regex
#   ./scripts/bench/bench-vs-tsgo.sh --filter 'utility-types' # Run only utility-types benchmarks
#   ./scripts/bench/bench-vs-tsgo.sh --rebuild          # Force rebuild of optimized binary
#   ./scripts/bench/bench-vs-tsgo.sh --prepare-only     # Build/install benchmark prerequisites, then exit
#
# The benchmark uses an isolated target directory (.target-bench/) to prevent
# interference from other cargo builds. The binary is built with the 'dist' profile
# which enables maximum optimizations (LTO=fat, codegen-units=1, stripped symbols).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BENCH_TIMEOUT_RUNNER="$SCRIPT_DIR/run-with-timeout.sh"

# Synthetic fixture generators are shared with the precommit microbench gate.
# shellcheck source=lib/synthetic-generators.sh
source "$SCRIPT_DIR/lib/synthetic-generators.sh"

# Lib assets: benchmark tsz with embedded (comment-stripped) lib files by default.
# Setting TSZ_LIB_DIR forces disk-based loading for explicit local overrides.
if [ -n "${TSZ_LIB_DIR:-}" ]; then
    export TSZ_LIB_DIR
elif [ -z "${TSZ_USE_EMBEDDED_LIBS+x}" ]; then
    export TSZ_USE_EMBEDDED_LIBS=1
else
    export TSZ_USE_EMBEDDED_LIBS
fi

# Dedicated target directory for benchmarks - isolated from dev builds.
BENCH_TARGET_DIR="$PROJECT_ROOT/.target-bench"
TSZ_OUTPUT_DIR="$BENCH_TARGET_DIR/dist"

# Compilers. TSZ can be provided by CI so benchmark runs reuse the already
# compiled binary instead of spending the bench job rebuilding it.
TSZ="${TSZ:-$TSZ_OUTPUT_DIR/tsz}"
TSZ_IS_OVERRIDE=false
if [ "$TSZ" != "$TSZ_OUTPUT_DIR/tsz" ]; then
    TSZ_IS_OVERRIDE=true
fi
TSGO="${TSGO:-}"
TSGO_TOOL_DIR="${TSGO_TOOL_DIR:-$BENCH_TARGET_DIR/tools/tsgo}"
TSGO_LOCAL_BIN="$TSGO_TOOL_DIR/node_modules/typescript/bin/tsc"
# tsc (TypeScript reference compiler)
TSC="${TSC:-}"
TSC_TOOL_DIR="${TSC_TOOL_DIR:-$BENCH_TARGET_DIR/tools/tsc}"
TSC_LOCAL_BIN="$TSC_TOOL_DIR/node_modules/typescript/bin/tsc"
# TypeScript 7 stable is the native compiler formerly published as the tsgo
# preview. Keep the benchmark label for continuity, but execute its `tsc` bin.
TSGO_NPM_SPEC="${TSGO_NPM_SPEC:-typescript@7.0.2}"
TSC_NPM_SPEC="${TSC_NPM_SPEC:-}"

# External benchmark fixtures (not checked into git)
EXTERNAL_BENCH_DIR="${EXTERNAL_BENCH_DIR:-$BENCH_TARGET_DIR/external}"
# shellcheck source=scripts/bench/project-fixtures.sh
source "$SCRIPT_DIR/project-fixtures.sh"
# shellcheck source=scripts/bench/lib/large-ts-repo-fixture.sh
source "$SCRIPT_DIR/lib/large-ts-repo-fixture.sh"
# Keep benchmark/CI project metadata aligned with a single source of truth.
tsz_sync_project_row_groups
if command -v node >/dev/null 2>&1; then
    tsz_validate_project_row_metadata
fi
# Project fixture pins live in project-fixtures.sh for benchmark/CI parity.
UTILITY_TYPES_DIR="$EXTERNAL_BENCH_DIR/utility-types"
TS_TOOLBELT_DIR="$EXTERNAL_BENCH_DIR/ts-toolbelt"
TS_ESSENTIALS_DIR="$EXTERNAL_BENCH_DIR/ts-essentials"
NEXTJS_DIR="$EXTERNAL_BENCH_DIR/next.js"
NEXT_APP_BENCH_DIR="${NEXT_APP_BENCH_DIR:-$EXTERNAL_BENCH_DIR/next-app-live}"
VITE_APP_BENCH_DIR="${VITE_APP_BENCH_DIR:-$EXTERNAL_BENCH_DIR/vite-vanilla-ts-live}"
RXJS_DIR="$EXTERNAL_BENCH_DIR/rxjs"
TYPE_FEST_DIR="$EXTERNAL_BENCH_DIR/type-fest"
ZOD_DIR="$EXTERNAL_BENCH_DIR/zod"
KYSELY_DIR="$EXTERNAL_BENCH_DIR/kysely"
VALIBOT_DIR="$EXTERNAL_BENCH_DIR/valibot"
MSW_DIR="$EXTERNAL_BENCH_DIR/msw"
COMLINK_DIR="$EXTERNAL_BENCH_DIR/comlink"
EFFECT_DIR="$EXTERNAL_BENCH_DIR/effect"
DRIZZLE_ORM_DIR="$EXTERNAL_BENCH_DIR/drizzle-orm"
TS_REST_DIR="$EXTERNAL_BENCH_DIR/ts-rest"
OFETCH_DIR="$EXTERNAL_BENCH_DIR/ofetch"
TS_PATTERN_DIR="$EXTERNAL_BENCH_DIR/ts-pattern"
RADASH_DIR="$EXTERNAL_BENCH_DIR/radash"
VALTIO_DIR="$EXTERNAL_BENCH_DIR/valtio"
SCULE_DIR="$EXTERNAL_BENCH_DIR/scule"
MITT_DIR="$EXTERNAL_BENCH_DIR/mitt"
CHANGE_CASE_DIR="$EXTERNAL_BENCH_DIR/change-case"
TINY_INVARIANT_DIR="$EXTERNAL_BENCH_DIR/tiny-invariant"
TS_BELT_DIR="$EXTERNAL_BENCH_DIR/ts-belt"
TS_EXTRAS_DIR="$EXTERNAL_BENCH_DIR/ts-extras"
SUPERJSON_DIR="$EXTERNAL_BENCH_DIR/superjson"
TRPC_DIR="$EXTERNAL_BENCH_DIR/trpc"
TANSTACK_QUERY_DIR="$EXTERNAL_BENCH_DIR/tanstack-query"
TANSTACK_ROUTER_DIR="$EXTERNAL_BENCH_DIR/tanstack-router"
ZUSTAND_DIR="$EXTERNAL_BENCH_DIR/zustand"
JOTAI_DIR="$EXTERNAL_BENCH_DIR/jotai"
FP_TS_DIR="$EXTERNAL_BENCH_DIR/fp-ts"
IO_TS_DIR="$EXTERNAL_BENCH_DIR/io-ts"
IMMER_DIR="$EXTERNAL_BENCH_DIR/immer"
REMEDA_DIR="$EXTERNAL_BENCH_DIR/remeda"
TS_MORPH_DIR="$EXTERNAL_BENCH_DIR/ts-morph"
ARKTYPE_DIR="$EXTERNAL_BENCH_DIR/arktype"
SUPERSTRUCT_DIR="$EXTERNAL_BENCH_DIR/superstruct"
RUNTYPES_DIR="$EXTERNAL_BENCH_DIR/runtypes"
HOTSCRIPT_DIR="$EXTERNAL_BENCH_DIR/hotscript"
TYPEBOX_DIR="$EXTERNAL_BENCH_DIR/typebox"
CLASS_TRANSFORMER_DIR="$EXTERNAL_BENCH_DIR/class-transformer"
TYPE_GRAPHQL_DIR="$EXTERNAL_BENCH_DIR/type-graphql"
NEVERTHROW_DIR="$EXTERNAL_BENCH_DIR/neverthrow"
XSTATE_DIR="$EXTERNAL_BENCH_DIR/xstate"
MOBX_DIR="$EXTERNAL_BENCH_DIR/mobx"
LARGE_TS_LOCAL_DIR="${HOME}/code/large-ts-repo"
LARGE_TS_DIR="$(tsz_large_ts_repo_default_dir "$EXTERNAL_BENCH_DIR")"
LARGE_TS_NODE_OPTIONS="${LARGE_TS_NODE_OPTIONS:---max-old-space-size=8192}"
# Deep project fixtures can exhaust Rust's default worker-thread stack before
# producing a benchmark result. Keep the default overrideable for local runs.
TSZ_RUST_MIN_STACK="${TSZ_RUST_MIN_STACK:-536870912}"

# Parse arguments
QUICK_MODE=false
JSON_OUTPUT=false
JSON_FILE=""
FILTER=""
FORCE_REBUILD=false
PREPARE_ONLY=false
NEXTJS_BENCHMARK_ENABLED="${NEXTJS_BENCHMARK_ENABLED:-0}"
TSZ_BENCH_INCLUDE_COMPILE_CANARIES="${TSZ_BENCH_INCLUDE_COMPILE_CANARIES:-0}"
BENCH_NPM_INSTALL_TIMEOUT="${BENCH_NPM_INSTALL_TIMEOUT:-900}"
BENCH_PGO_TSZ_TIMEOUT="${BENCH_PGO_TSZ_TIMEOUT:-900}"
BENCH_CARGO_BUILD_TIMEOUT="${BENCH_CARGO_BUILD_TIMEOUT:-1200}"
BENCH_PGO_MARKER="$TSZ_OUTPUT_DIR/.bench-pgo-optimized"
declare -a BENCH_PGO_TRAINING_INPUTS=()
declare -a BENCH_PGO_TRAINING_FAILED_INPUTS=()
while [[ $# -gt 0 ]]; do
    case $1 in
        --quick) QUICK_MODE=true; shift ;;
        --json) JSON_OUTPUT=true; shift ;;
        --json-file) JSON_OUTPUT=true; JSON_FILE="$2"; shift 2 ;;
        --filter) FILTER="$2"; shift 2 ;;
        --rebuild) FORCE_REBUILD=true; shift ;;
        --prepare-only) PREPARE_ONLY=true; shift ;;
        --help|-h)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --quick     Quick smoke test (fewer runs, fewer files)"
            echo "  --json      Export results to JSON (default path: artifacts/bench-vs-tsgo-<timestamp>.json)"
            echo "  --json-file Write JSON results to a specific path"
            echo "  --filter    Run only tests matching regex (e.g., --filter 'BCT|CFA')"
            echo "  --rebuild   Force rebuild of tsz binary (ensures fresh optimized build)"
            echo "  --prepare-only Build/install benchmark prerequisites and exit"
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
            echo "  TSZ=<path>             Use a specific tsz binary (skip benchmark build)"
            echo "  TSZ_LIB_DIR=<path>     Override tsz lib assets (default: embedded)"
            echo "  TSZ_BENCH_INCLUDE_COMPILE_CANARIES=1 Include known-red project rows in local full runs"
            echo "  UTILITY_TYPES_REF=<sha> Override pinned utility-types commit"
            echo "  TS_TOOLBELT_REF=<sha>  Override pinned ts-toolbelt commit"
            echo "  TS_ESSENTIALS_REF=<sha> Override pinned ts-essentials commit"
            echo "  NEXTJS_REF=<sha>       Override pinned next.js commit"
            echo "  VITE_APP_BENCH_DIR=<path> Override generated Vite fixture directory"
            echo "  BENCH_PGO=0            Skip PGO training (default: 1 when llvm-profdata is available)"
            echo "  BENCH_REQUIRE_PGO=1    Fail instead of falling back to a non-PGO build (default: 0)"
            echo "  BENCH_PGO_CACHE=0      Don't reuse the cached profdata across runs (default: 1)"
            echo "  BENCH_PGO_FETCH_UTILITY_TYPES=0  Don't fetch utility-types for PGO training (default: 1)"
            echo "  BENCH_PGO_TSZ_TIMEOUT=<seconds>  Timeout for each PGO training compiler invocation (default: 900)"
            echo "  BENCH_CARGO_BUILD_TIMEOUT=<seconds>  Timeout for each cargo build in check_prerequisites (default: 1200)"
            echo "  TSZ_BENCH_PROJECT_SLOWDOWN_FAILURE_FACTOR=<factor> Mark green project rows slower than tsgo by this factor as timing failures (default: 1.5; 0 disables)"
            echo "  BENCH_PGO_FETCH_CORE_PROJECTS=1  Fetch/train ts-toolbelt/ts-essentials during PGO (default: 0; slower)"
            echo "  BENCH_PGO_SYNTHETIC=0  Don't train PGO on generated benchmark stress cases (default: 1)"
            echo "  BENCH_PGO_PANIC_UNWIND=1  Build the trainer with panic=unwind for crashy inputs (default: 0)"
            echo "  BENCH_PGO_EXTRA_INPUTS=<path[:path]>  Extra .ts or tsconfig files to feed the PGO trainer"
            echo "  BENCH_PGO_VERBOSE=1    Print per-input wall time during PGO Step 2"
            echo "  BENCH_RUST_TARGET_CPU=<cpu>  Rust target-cpu for bench builds (default: native; CI pins x86-64-v3)"
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

# shellcheck source=scripts/bench/lib/bench-vs-tsgo-prereqs.sh
source "$SCRIPT_DIR/lib/bench-vs-tsgo-prereqs.sh"

# Persist the project-file-stats line-count cache beside the other
# run-surviving bench subdirectories (dist/, tools/, external/) instead of the
# per-run temp dir, so unchanged fixture sources are line-counted at most once
# across row invocations (issue #10923). Resolved once here via the prereqs
# helper so the default lives in a single place.
TSZ_PROJECT_FILE_STATS_CACHE_DIR="$(bench_project_file_stats_cache_dir)"
export TSZ_PROJECT_FILE_STATS_CACHE_DIR

# shellcheck source=scripts/bench/lib/bench-vs-tsgo-results.sh
source "$SCRIPT_DIR/lib/bench-vs-tsgo-results.sh"

should_run_compile_canary_project() {
    if [ -n "$FILTER" ]; then
        return 0
    fi
    [ "$TSZ_BENCH_INCLUDE_COMPILE_CANARIES" = "1" ]
}

# shellcheck source=scripts/bench/lib/application-benchmarks.sh
source "$SCRIPT_DIR/lib/application-benchmarks.sh"


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
            '/packages/next/package.json' \
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

    tsz_write_nextjs_bench_globals "$NEXTJS_DIR/packages/next/tsz-bench-globals.d.ts"
    tsz_write_nextjs_external_module "$NEXTJS_DIR/packages/next/tsz-bench-external-module.d.ts"
    tsz_write_nextjs_config "$NEXTJS_DIR/packages/next/tsconfig.tsz-bench.json"
}

ensure_next_app_benchmark_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"

    if ! command -v npm &>/dev/null; then
        echo -e "${RED}✗ npm not found. Install npm to generate the fresh Next.js benchmark app.${NC}"
        return 1
    fi

    echo -e "${CYAN}Generating fresh Next.js benchmark app...${NC}"
    node "$SCRIPT_DIR/generate-next-app-fixture.mjs" "$NEXT_APP_BENCH_DIR"
}

ensure_vite_app_benchmark_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"

    if ! command -v npm &>/dev/null; then
        echo -e "${RED}✗ npm not found. Install npm to generate the fresh Vite benchmark app.${NC}"
        return 1
    fi

    echo -e "${CYAN}Generating fresh Vite vanilla TypeScript benchmark app...${NC}"
    node "$SCRIPT_DIR/generate-vite-app-fixture.mjs" "$VITE_APP_BENCH_DIR"
}

ensure_utility_types_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "utility-types" "$UTILITY_TYPES_REPO" "$UTILITY_TYPES_REF" "$UTILITY_TYPES_DIR" 1 || return $?

    # Rewrite the generated flat tsconfig every run. External fixture clones
    # are cached across benchmark jobs, and stale generated configs can keep
    # obsolete include/exclude rules after this script changes.
    # Create flat tsconfig for project-mode benchmarking:
    # - excludes spec/snap test files (need @types/jest)
    # - uses skipLibCheck + types:[] to avoid needing external type deps
    # - uses ES2015 target (ES5 is deprecated in TS 6+)
    local flat_tsconfig="$UTILITY_TYPES_DIR/tsconfig.flat.json"
    tsz_write_utility_types_config "$flat_tsconfig"
}

ensure_ts_toolbelt_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "ts-toolbelt" "$TS_TOOLBELT_REPO" "$TS_TOOLBELT_REF" "$TS_TOOLBELT_DIR" 1 || return $?

    # Rewrite the generated flat tsconfig every run; fixture clones are cached
    # across jobs and must pick up script-owned config changes.
    # Create flat tsconfig for project-mode benchmarking:
    # - sources only (excludes tests/scripts which need external deps)
    # - removes deprecated/unsupported options (suppressImplicitAnyIndexErrors, watch)
    # - uses skipLibCheck + types:[] to avoid needing external type deps
    local flat_tsconfig="$TS_TOOLBELT_DIR/tsconfig.flat.json"
    tsz_write_ts_toolbelt_config "$flat_tsconfig"
}

ensure_ts_essentials_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "ts-essentials" "$TS_ESSENTIALS_REPO" "$TS_ESSENTIALS_REF" "$TS_ESSENTIALS_DIR" 1 || return $?

    # Rewrite the generated flat tsconfig every run; fixture clones are cached
    # across jobs and must pick up script-owned config changes.
    # Create flat tsconfig for project-mode benchmarking:
    # - lib sources only (excludes test dir which needs conditional-type-checks)
    # - uses es2018 lib (covers esnext.asynciterable from original config)
    # - uses skipLibCheck to avoid needing external type deps
    local flat_tsconfig="$TS_ESSENTIALS_DIR/tsconfig.flat.json"
    tsz_write_ts_essentials_config "$flat_tsconfig"
}

# ─── Real-world fixture: rxjs ───────────────────────────────────────────────
ensure_rxjs_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "rxjs" "$RXJS_REPO" "$RXJS_REF" "$RXJS_DIR" 1 || return $?
    # rxjs has been a monorepo since the v8 work — `src/internal` moved to
    # `packages/rxjs/src/internal`. Detect both layouts.
    local rxjs_src_root
    rxjs_src_root="$(tsz_rxjs_src_root "$RXJS_DIR")"
    # Rewrite the generated flat tsconfig every run; fixture clones are cached
    # across jobs and must pick up script-owned config changes.
    local flat_tsconfig="$RXJS_DIR/tsconfig.flat.json"
    tsz_write_rxjs_config "$flat_tsconfig" "$rxjs_src_root"
}

# ─── Real-world fixture: type-fest ──────────────────────────────────────────
ensure_type_fest_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "type-fest" "$TYPE_FEST_REPO" "$TYPE_FEST_REF" "$TYPE_FEST_DIR" 1 || return $?
    # Rewrite the generated flat tsconfig every run; fixture clones are cached
    # across jobs and must pick up script-owned config changes.
    local flat_tsconfig="$TYPE_FEST_DIR/tsconfig.flat.json"
    tsz_write_type_fest_config "$flat_tsconfig"
}

# ─── Real-world fixture: zod ────────────────────────────────────────────────
ensure_zod_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "zod" "$ZOD_REPO" "$ZOD_REF" "$ZOD_DIR" 1 || return $?
    # Rewrite the generated flat tsconfig every run; fixture clones are cached
    # across jobs and must pick up script-owned config changes.
    local flat_tsconfig="$ZOD_DIR/tsconfig.flat.json"
    tsz_write_zod_config "$flat_tsconfig"
}

# ─── Real-world fixture: kysely (extreme type-level SQL inference) ─────────
ensure_kysely_fixture() {
    mkdir -p "$EXTERNAL_BENCH_DIR"
    tsz_ensure_git_fixture "kysely" "$KYSELY_REPO" "$KYSELY_REF" "$KYSELY_DIR" 1 || return $?
    local flat_tsconfig="$KYSELY_DIR/tsconfig.flat.json"
    local bench_globals="$KYSELY_DIR/tsz-bench-globals.d.ts"
    tsz_write_kysely_globals "$bench_globals"
    # Rewrite the generated flat tsconfig every run; fixture clones are cached
    # across jobs and must pick up script-owned config changes.
    tsz_write_kysely_config "$flat_tsconfig"
}

# shellcheck source=scripts/bench/lib/bench-vs-tsgo-project-fixtures.sh
source "$SCRIPT_DIR/lib/bench-vs-tsgo-project-fixtures.sh"

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
    ensure_utility_types_fixture || return $?
    echo -e "${GREEN}✓${NC} utility-types pinned at $(git -C "$UTILITY_TYPES_DIR" rev-parse --short HEAD)"

    # Use project's tsconfig lib settings (dom, es2017) for fair comparison
    # Without this, tsz loads all default libs which is slower and doesn't match tsgo's behavior
    local lib_args="--lib dom,es2017"

    local files
    if use_quick_subset_for "utility-types/index.ts"; then
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
    ensure_ts_toolbelt_fixture || return $?
    echo -e "${GREEN}✓${NC} ts-toolbelt pinned at $(git -C "$TS_TOOLBELT_DIR" rev-parse --short HEAD)"

    # ts-toolbelt needs esnext+dom libs (per its tsconfig), otherwise tsc can't
    # resolve Symbol/Map/Promise etc.
    local lib_args="--lib esnext,dom"

    local files
    if use_quick_subset_for "ts-toolbelt/Iteration/Iteration.ts"; then
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
    ensure_ts_essentials_fixture || return $?
    echo -e "${GREEN}✓${NC} ts-essentials pinned at $(git -C "$TS_ESSENTIALS_DIR" rev-parse --short HEAD)"

    # ts-essentials needs es2018 libs (per its tsconfig) for Map, Symbol, etc.
    local lib_args="--lib es2018"

    local files
    if use_quick_subset_for "ts-essentials/paths.ts"; then
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

run_utility_types_project_benchmarks() {
    if ! is_benchmark_selected "utility-types-project"; then
        return
    fi

    print_header "Real-world External Project - utility-types (whole project)"
    ensure_utility_types_fixture || return $?
    echo -e "${GREEN}✓${NC} utility-types pinned at $(git -C "$UTILITY_TYPES_DIR" rev-parse --short HEAD)"

    local tsconfig="$UTILITY_TYPES_DIR/tsconfig.flat.json"
    local src_dir="$UTILITY_TYPES_DIR/src"

    if [ ! -f "$tsconfig" ]; then
        echo -e "${RED}✗ tsconfig not found: $tsconfig${NC}"
        return
    fi

    run_project_benchmark "utility-types-project" "$tsconfig" "$src_dir"
    echo
}

run_ts_toolbelt_project_benchmarks() {
    if ! is_benchmark_selected "ts-toolbelt-project"; then
        return
    fi

    print_header "Real-world External Project - ts-toolbelt (whole project, 242 type-level files)"
    ensure_ts_toolbelt_fixture || return $?
    echo -e "${GREEN}✓${NC} ts-toolbelt pinned at $(git -C "$TS_TOOLBELT_DIR" rev-parse --short HEAD)"

    local tsconfig="$TS_TOOLBELT_DIR/tsconfig.flat.json"
    local src_dir="$TS_TOOLBELT_DIR/sources"

    if [ ! -f "$tsconfig" ]; then
        echo -e "${RED}✗ tsconfig not found: $tsconfig${NC}"
        return
    fi

    run_project_benchmark "ts-toolbelt-project" "$tsconfig" "$src_dir"
    echo
}

run_ts_essentials_project_benchmarks() {
    if ! is_benchmark_selected "ts-essentials-project"; then
        return
    fi

    print_header "Real-world External Project - ts-essentials (whole project, 95 type utility files)"
    ensure_ts_essentials_fixture || return $?
    echo -e "${GREEN}✓${NC} ts-essentials pinned at $(git -C "$TS_ESSENTIALS_DIR" rev-parse --short HEAD)"

    local tsconfig="$TS_ESSENTIALS_DIR/tsconfig.flat.json"
    local src_dir="$TS_ESSENTIALS_DIR/lib"

    if [ ! -f "$tsconfig" ]; then
        echo -e "${RED}✗ tsconfig not found: $tsconfig${NC}"
        return
    fi

    run_project_benchmark "ts-essentials-project" "$tsconfig" "$src_dir"
    echo
}

run_rxjs_project_benchmarks() {
    if ! is_benchmark_selected "rxjs-project"; then
        return
    fi

    print_header "Real-world External Project - rxjs (source parse with noCheck)"
    ensure_rxjs_fixture || return $?
    echo -e "${GREEN}✓${NC} rxjs pinned at $(git -C "$RXJS_DIR" rev-parse --short HEAD)"

    local tsconfig="$RXJS_DIR/tsconfig.flat.json"
    local src_dir
    if [ -d "$RXJS_DIR/packages/rxjs/src/internal" ]; then
        src_dir="$RXJS_DIR/packages/rxjs/src/internal"
    else
        src_dir="$RXJS_DIR/src/internal"
    fi

    if [ ! -f "$tsconfig" ]; then
        echo -e "${RED}✗ tsconfig not found: $tsconfig${NC}"
        return
    fi

    run_project_benchmark "rxjs-project" "$tsconfig" "$src_dir"
    echo
}

run_type_fest_project_benchmarks() {
    if ! is_benchmark_selected "type-fest-project"; then
        return
    fi

    print_header "Real-world External Project - type-fest (broad utility-type surface)"
    ensure_type_fest_fixture || return $?
    echo -e "${GREEN}✓${NC} type-fest pinned at $(git -C "$TYPE_FEST_DIR" rev-parse --short HEAD)"

    local tsconfig="$TYPE_FEST_DIR/tsconfig.flat.json"
    local src_dir="$TYPE_FEST_DIR/source"

    if [ ! -f "$tsconfig" ]; then
        echo -e "${RED}✗ tsconfig not found: $tsconfig${NC}"
        return
    fi

    run_project_benchmark "type-fest-project" "$tsconfig" "$src_dir"
    echo
}

run_zod_project_benchmarks() {
    if ! is_benchmark_selected "zod-project"; then
        return
    fi

    print_header "Real-world External Project - zod (deep z.infer<typeof> schema inference)"
    ensure_zod_fixture || return $?
    echo -e "${GREEN}✓${NC} zod pinned at $(git -C "$ZOD_DIR" rev-parse --short HEAD)"

    # zod v3 lives in src/, zod v4 monorepo lives in packages/zod/src/.
    # Pick whichever exists so the bench works on either layout.
    local tsconfig="$ZOD_DIR/tsconfig.flat.json"
    local src_dir
    if [ -d "$ZOD_DIR/packages/zod/src" ]; then
        src_dir="$ZOD_DIR/packages/zod/src"
    else
        src_dir="$ZOD_DIR/src"
    fi

    if [ ! -f "$tsconfig" ]; then
        echo -e "${RED}✗ tsconfig not found: $tsconfig${NC}"
        return
    fi

    run_project_benchmark "zod-project" "$tsconfig" "$src_dir"
    echo
}

run_kysely_project_benchmarks() {
    if ! is_benchmark_selected "kysely-project"; then
        return
    fi

    print_header "Real-world External Project - kysely (extreme type-level SQL inference)"
    ensure_kysely_fixture || return $?
    echo -e "${GREEN}✓${NC} kysely pinned at $(git -C "$KYSELY_DIR" rev-parse --short HEAD)"

    local tsconfig="$KYSELY_DIR/tsconfig.flat.json"
    local src_dir="$KYSELY_DIR/src"

    if [ ! -f "$tsconfig" ]; then
        echo -e "${RED}✗ tsconfig not found: $tsconfig${NC}"
        return
    fi

    run_project_benchmark "kysely-project" "$tsconfig" "$src_dir"
    echo
}

run_valibot_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "valibot-project"; then
        return
    fi

    print_header "Real-world External Project - Valibot"
    ensure_valibot_fixture || return 1
    local tsconfig="$VALIBOT_DIR/tsconfig.tsz-bench.json"
    local src_dir="$VALIBOT_DIR/library/src"
    tsz_write_valibot_config "$tsconfig"
    run_project_benchmark "valibot-project" "$tsconfig" "$src_dir"
    echo
}

run_msw_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "msw-project"; then
        return
    fi

    print_header "Real-world External Project - MSW"
    ensure_msw_fixture || return 1
    local tsconfig="$MSW_DIR/tsconfig.tsz-bench.json"
    local src_dir="$MSW_DIR/src"
    tsz_write_msw_config "$tsconfig"
    run_project_benchmark "msw-project" "$tsconfig" "$src_dir"
    echo
}

run_comlink_project_benchmarks() {
    if ! is_benchmark_selected "comlink-project"; then
        return
    fi

    print_header "Real-world External Project - Comlink"
    ensure_comlink_fixture || return 1
    local tsconfig="$COMLINK_DIR/tsconfig.tsz-bench.json"
    local src_dir="$COMLINK_DIR/src"
    tsz_write_comlink_config "$tsconfig"
    run_project_benchmark "comlink-project" "$tsconfig" "$src_dir"
    echo
}

run_effect_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "effect-project"; then
        return
    fi

    print_header "Real-world External Project - Effect"
    ensure_effect_fixture || return 1
    local tsconfig="$EFFECT_DIR/tsconfig.tsz-bench.json"
    local src_dir="$EFFECT_DIR/packages/effect/src"
    tsz_write_effect_config "$tsconfig"
    run_project_benchmark "effect-project" "$tsconfig" "$src_dir"
    echo
}

run_drizzle_orm_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "drizzle-orm-project"; then
        return
    fi

    print_header "Real-world External Project - Drizzle ORM"
    ensure_drizzle_orm_fixture || return 1
    local tsconfig="$DRIZZLE_ORM_DIR/tsconfig.tsz-bench.json"
    local src_dir="$DRIZZLE_ORM_DIR/drizzle-orm/src"
    tsz_write_drizzle_orm_config "$tsconfig"
    run_project_benchmark "drizzle-orm-project" "$tsconfig" "$src_dir"
    echo
}

run_ts_rest_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "ts-rest-project"; then
        return
    fi

    print_header "Real-world External Project - ts-rest"
    ensure_ts_rest_fixture || return 1
    local tsconfig="$TS_REST_DIR/tsconfig.tsz-bench.json"
    local src_dir="$TS_REST_DIR/libs/ts-rest/core/src"
    tsz_write_ts_rest_config "$tsconfig"
    run_project_benchmark "ts-rest-project" "$tsconfig" "$src_dir"
    echo
}

run_ofetch_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "ofetch-project"; then
        return
    fi

    print_header "Real-world External Project - ofetch"
    ensure_ofetch_fixture || return 1
    local tsconfig="$OFETCH_DIR/tsconfig.tsz-bench.json"
    local src_dir="$OFETCH_DIR/src"
    tsz_write_ofetch_config "$tsconfig"
    run_project_benchmark "ofetch-project" "$tsconfig" "$src_dir"
    echo
}

run_ts_pattern_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "ts-pattern-project"; then
        return
    fi

    print_header "Real-world External Project - ts-pattern"
    ensure_ts_pattern_fixture || return 1
    local tsconfig="$TS_PATTERN_DIR/tsconfig.tsz-bench.json"
    local src_dir="$TS_PATTERN_DIR/src"
    tsz_write_ts_pattern_config "$tsconfig"
    run_project_benchmark "ts-pattern-project" "$tsconfig" "$src_dir"
    echo
}

run_radash_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "radash-project"; then
        return
    fi

    print_header "Real-world External Project - radash"
    ensure_radash_fixture || return 1
    local tsconfig="$RADASH_DIR/tsconfig.tsz-bench.json"
    local src_dir="$RADASH_DIR/src"
    tsz_write_radash_config "$tsconfig"
    run_project_benchmark "radash-project" "$tsconfig" "$src_dir"
    echo
}

run_valtio_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "valtio-project"; then
        return
    fi

    print_header "Real-world External Project - valtio"
    ensure_valtio_fixture || return 1
    local tsconfig="$VALTIO_DIR/tsconfig.tsz-bench.json"
    local src_dir="$VALTIO_DIR/src"
    tsz_write_valtio_config "$tsconfig"
    run_project_benchmark "valtio-project" "$tsconfig" "$src_dir"
    echo
}

run_scule_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "scule-project"; then
        return
    fi

    print_header "Real-world External Project - scule"
    ensure_scule_fixture || return 1
    local tsconfig="$SCULE_DIR/tsconfig.tsz-bench.json"
    local src_dir="$SCULE_DIR/src"
    tsz_write_scule_config "$tsconfig"
    run_project_benchmark "scule-project" "$tsconfig" "$src_dir"
    echo
}

run_mitt_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "mitt-project"; then
        return
    fi

    print_header "Real-world External Project - mitt"
    ensure_mitt_fixture || return 1
    local tsconfig="$MITT_DIR/tsconfig.tsz-bench.json"
    local src_dir="$MITT_DIR/src"
    tsz_write_mitt_config "$tsconfig"
    run_project_benchmark "mitt-project" "$tsconfig" "$src_dir"
    echo
}

run_change_case_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "change-case-project"; then
        return
    fi

    print_header "Real-world External Project - change-case"
    ensure_change_case_fixture || return 1
    local tsconfig="$CHANGE_CASE_DIR/tsconfig.tsz-bench.json"
    local src_dir="$CHANGE_CASE_DIR/packages/change-case/src"
    tsz_write_change_case_config "$tsconfig"
    run_project_benchmark "change-case-project" "$tsconfig" "$src_dir"
    echo
}

run_tiny_invariant_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "tiny-invariant-project"; then
        return
    fi

    print_header "Real-world External Project - tiny-invariant"
    ensure_tiny_invariant_fixture || return 1
    local tsconfig="$TINY_INVARIANT_DIR/tsconfig.tsz-bench.json"
    local src_dir="$TINY_INVARIANT_DIR/src"
    tsz_write_tiny_invariant_config "$tsconfig"
    run_project_benchmark "tiny-invariant-project" "$tsconfig" "$src_dir"
    echo
}

run_ts_belt_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "ts-belt-project"; then
        return
    fi

    print_header "Real-world External Project - ts-belt"
    ensure_ts_belt_fixture || return 1
    local tsconfig="$TS_BELT_DIR/tsconfig.tsz-bench.json"
    local src_dir="$TS_BELT_DIR/src"
    tsz_write_ts_belt_config "$tsconfig"
    run_project_benchmark "ts-belt-project" "$tsconfig" "$src_dir"
    echo
}

run_ts_extras_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "ts-extras-project"; then
        return
    fi

    print_header "Real-world External Project - ts-extras"
    ensure_ts_extras_fixture || return 1
    local tsconfig="$TS_EXTRAS_DIR/tsconfig.tsz-bench.json"
    local src_dir="$TS_EXTRAS_DIR/source"
    tsz_write_ts_extras_config "$tsconfig"
    run_project_benchmark "ts-extras-project" "$tsconfig" "$src_dir"
    echo
}

run_superjson_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "superjson-project"; then
        return
    fi

    print_header "Real-world External Project - superjson"
    ensure_superjson_fixture || return 1
    local tsconfig="$SUPERJSON_DIR/tsconfig.tsz-bench.json"
    local src_dir="$SUPERJSON_DIR/src"
    tsz_write_superjson_config "$tsconfig"
    run_project_benchmark "superjson-project" "$tsconfig" "$src_dir"
    echo
}

run_trpc_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "trpc-project"; then
        return
    fi

    print_header "Real-world External Project - trpc"
    ensure_trpc_fixture || return 1
    local tsconfig="$TRPC_DIR/tsconfig.tsz-bench.json"
    local src_dir="$TRPC_DIR/packages/server/src"
    tsz_write_trpc_config "$tsconfig"
    run_project_benchmark "trpc-project" "$tsconfig" "$src_dir"
    echo
}

run_tanstack_query_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "tanstack-query-project"; then
        return
    fi

    print_header "Real-world External Project - tanstack-query"
    ensure_tanstack_query_fixture || return 1
    local tsconfig="$TANSTACK_QUERY_DIR/tsconfig.tsz-bench.json"
    local src_dir="$TANSTACK_QUERY_DIR/packages/query-core/src"
    tsz_write_tanstack_query_config "$tsconfig"
    run_project_benchmark "tanstack-query-project" "$tsconfig" "$src_dir"
    echo
}

run_tanstack_router_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "tanstack-router-project"; then
        return
    fi

    print_header "Real-world External Project - tanstack-router"
    ensure_tanstack_router_fixture || return 1
    local tsconfig="$TANSTACK_ROUTER_DIR/tsconfig.tsz-bench.json"
    local src_dir="$TANSTACK_ROUTER_DIR/packages/router-core/src"
    tsz_write_tanstack_router_config "$tsconfig"
    run_project_benchmark "tanstack-router-project" "$tsconfig" "$src_dir"
    echo
}

run_zustand_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "zustand-project"; then
        return
    fi

    print_header "Real-world External Project - zustand"
    ensure_zustand_fixture || return 1
    local tsconfig="$ZUSTAND_DIR/tsconfig.tsz-bench.json"
    local src_dir="$ZUSTAND_DIR/src"
    tsz_write_zustand_config "$tsconfig"
    run_project_benchmark "zustand-project" "$tsconfig" "$src_dir"
    echo
}

run_jotai_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "jotai-project"; then
        return
    fi

    print_header "Real-world External Project - jotai"
    ensure_jotai_fixture || return 1
    local tsconfig="$JOTAI_DIR/tsconfig.tsz-bench.json"
    local src_dir="$JOTAI_DIR/src"
    tsz_write_jotai_config "$tsconfig"
    run_project_benchmark "jotai-project" "$tsconfig" "$src_dir"
    echo
}

run_fp_ts_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "fp-ts-project"; then
        return
    fi

    print_header "Real-world External Project - fp-ts"
    ensure_fp_ts_fixture || return 1
    local tsconfig="$FP_TS_DIR/tsconfig.tsz-bench.json"
    local src_dir="$FP_TS_DIR/src"
    tsz_write_fp_ts_config "$tsconfig"
    run_project_benchmark "fp-ts-project" "$tsconfig" "$src_dir"
    echo
}

run_io_ts_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "io-ts-project"; then
        return
    fi

    print_header "Real-world External Project - io-ts"
    ensure_io_ts_fixture || return 1
    local tsconfig="$IO_TS_DIR/tsconfig.tsz-bench.json"
    local src_dir="$IO_TS_DIR/src"
    tsz_write_io_ts_config "$tsconfig"
    run_project_benchmark "io-ts-project" "$tsconfig" "$src_dir"
    echo
}

run_immer_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "immer-project"; then
        return
    fi

    print_header "Real-world External Project - immer"
    ensure_immer_fixture || return 1
    local tsconfig="$IMMER_DIR/tsconfig.tsz-bench.json"
    local src_dir="$IMMER_DIR/src"
    tsz_write_immer_config "$tsconfig"
    run_project_benchmark "immer-project" "$tsconfig" "$src_dir"
    echo
}

run_remeda_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "remeda-project"; then
        return
    fi

    print_header "Real-world External Project - remeda"
    ensure_remeda_fixture || return 1
    local tsconfig="$REMEDA_DIR/tsconfig.tsz-bench.json"
    local src_dir="$REMEDA_DIR/packages/remeda/src"
    tsz_write_remeda_config "$tsconfig"
    run_project_benchmark "remeda-project" "$tsconfig" "$src_dir"
    echo
}

run_ts_morph_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "ts-morph-project"; then
        return
    fi

    print_header "Real-world External Project - ts-morph"
    ensure_ts_morph_fixture || return 1
    local tsconfig="$TS_MORPH_DIR/tsconfig.tsz-bench.json"
    local src_dir="$TS_MORPH_DIR/packages/ts-morph/src"
    tsz_write_ts_morph_config "$tsconfig"
    run_project_benchmark "ts-morph-project" "$tsconfig" "$src_dir"
    echo
}

run_arktype_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "arktype-project"; then
        return
    fi

    print_header "Real-world External Project - arktype"
    ensure_arktype_fixture || return 1
    local tsconfig="$ARKTYPE_DIR/tsconfig.tsz-bench.json"
    local src_dir="$ARKTYPE_DIR/ark/type"
    tsz_write_arktype_config "$tsconfig"
    run_project_benchmark "arktype-project" "$tsconfig" "$src_dir"
    echo
}

run_superstruct_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "superstruct-project"; then
        return
    fi

    print_header "Real-world External Project - superstruct"
    ensure_superstruct_fixture || return 1
    local tsconfig="$SUPERSTRUCT_DIR/tsconfig.tsz-bench.json"
    local src_dir="$SUPERSTRUCT_DIR/src"
    tsz_write_superstruct_config "$tsconfig"
    run_project_benchmark "superstruct-project" "$tsconfig" "$src_dir"
    echo
}

run_runtypes_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "runtypes-project"; then
        return
    fi

    print_header "Real-world External Project - runtypes"
    ensure_runtypes_fixture || return 1
    local tsconfig="$RUNTYPES_DIR/tsconfig.tsz-bench.json"
    local src_dir="$RUNTYPES_DIR/src"
    tsz_write_runtypes_config "$tsconfig"
    run_project_benchmark "runtypes-project" "$tsconfig" "$src_dir"
    echo
}

run_hotscript_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "hotscript-project"; then
        return
    fi

    print_header "Real-world External Project - hotscript"
    ensure_hotscript_fixture || return 1
    local tsconfig="$HOTSCRIPT_DIR/tsconfig.tsz-bench.json"
    local src_dir="$HOTSCRIPT_DIR/src"
    tsz_write_hotscript_config "$tsconfig"
    run_project_benchmark "hotscript-project" "$tsconfig" "$src_dir"
    echo
}

run_typebox_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "typebox-project"; then
        return
    fi

    print_header "Real-world External Project - typebox"
    ensure_typebox_fixture || return 1
    local tsconfig="$TYPEBOX_DIR/tsconfig.tsz-bench.json"
    local src_dir="$TYPEBOX_DIR/src"
    tsz_write_typebox_config "$tsconfig"
    run_project_benchmark "typebox-project" "$tsconfig" "$src_dir"
    echo
}

run_class_transformer_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "class-transformer-project"; then
        return
    fi

    print_header "Real-world External Project - class-transformer"
    ensure_class_transformer_fixture || return 1
    local tsconfig="$CLASS_TRANSFORMER_DIR/tsconfig.tsz-bench.json"
    local src_dir="$CLASS_TRANSFORMER_DIR/src"
    tsz_write_class_transformer_config "$tsconfig"
    run_project_benchmark "class-transformer-project" "$tsconfig" "$src_dir"
    echo
}

run_type_graphql_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "type-graphql-project"; then
        return
    fi

    print_header "Real-world External Project - type-graphql"
    ensure_type_graphql_fixture || return 1
    local tsconfig="$TYPE_GRAPHQL_DIR/tsconfig.tsz-bench.json"
    local src_dir="$TYPE_GRAPHQL_DIR/src"
    tsz_write_type_graphql_config "$tsconfig"
    run_project_benchmark "type-graphql-project" "$tsconfig" "$src_dir"
    echo
}

run_neverthrow_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "neverthrow-project"; then
        return
    fi

    print_header "Real-world External Project - neverthrow"
    ensure_neverthrow_fixture || return 1
    local tsconfig="$NEVERTHROW_DIR/tsconfig.tsz-bench.json"
    local src_dir="$NEVERTHROW_DIR/src"
    tsz_write_neverthrow_config "$tsconfig"
    run_project_benchmark "neverthrow-project" "$tsconfig" "$src_dir"
    echo
}

run_xstate_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "xstate-project"; then
        return
    fi

    print_header "Real-world External Project - xstate"
    ensure_xstate_fixture || return 1
    local tsconfig="$XSTATE_DIR/tsconfig.tsz-bench.json"
    local src_dir="$XSTATE_DIR/packages/core/src"
    tsz_write_xstate_config "$tsconfig"
    run_project_benchmark "xstate-project" "$tsconfig" "$src_dir"
    echo
}

run_mobx_project_benchmarks() {
    should_run_compile_canary_project || return 0
    if ! is_benchmark_selected "mobx-project"; then
        return
    fi

    print_header "Real-world External Project - mobx"
    ensure_mobx_fixture || return 1
    local tsconfig="$MOBX_DIR/tsconfig.tsz-bench.json"
    local src_dir="$MOBX_DIR/packages/mobx/src"
    tsz_write_mobx_config "$tsconfig"
    run_project_benchmark "mobx-project" "$tsconfig" "$src_dir"
    echo
}

run_nextjs_benchmarks() {
    # nextjs fixture is gated off by default (kill-switch for an unstable sparse fixture),
    # but an explicit --filter must still reach the row for local debugging.
    if [ "$NEXTJS_BENCHMARK_ENABLED" != "1" ] && [ -z "$FILTER" ]; then
        return
    fi

    if ! is_benchmark_selected "nextjs"; then
        return
    fi

    print_header "Real-world External Project - next.js (full project)"
    ensure_nextjs_fixture || return $?
    echo -e "${GREEN}✓${NC} next.js pinned at $(git -C "$NEXTJS_DIR" rev-parse --short HEAD)"

    local tsconfig="$NEXTJS_DIR/packages/next/tsconfig.tsz-bench.json"
    local src_dir="$NEXTJS_DIR/packages/next/src"

    if [ ! -f "$tsconfig" ]; then
        echo -e "${RED}✗ tsconfig not found: $tsconfig${NC}"
        return
    fi

    run_project_benchmark "nextjs" "$tsconfig" "$src_dir"
    echo
}

run_next_app_project_benchmarks() {
    if ! is_benchmark_selected "nextjs-fresh-app"; then
        return
    fi

    print_header "Generated Project - fresh Next.js app"
    ensure_next_app_benchmark_fixture || return 1

    local tsconfig="$NEXT_APP_BENCH_DIR/tsconfig.json"
    local src_dir="$NEXT_APP_BENCH_DIR"

    if [ ! -f "$tsconfig" ]; then
        echo -e "${RED}✗ tsconfig not found: $tsconfig${NC}"
        return
    fi

    run_project_benchmark "nextjs-fresh-app" "$tsconfig" "$src_dir"
    echo
}

run_vite_app_project_benchmarks() {
    if ! is_benchmark_selected "vite-vanilla-ts-app"; then
        return
    fi

    print_header "Generated Project - fresh Vite vanilla TypeScript app"
    ensure_vite_app_benchmark_fixture || return 1

    local tsconfig="$VITE_APP_BENCH_DIR/tsconfig.json"
    local src_dir="$VITE_APP_BENCH_DIR"

    if [ ! -f "$tsconfig" ]; then
        echo -e "${RED}✗ tsconfig not found: $tsconfig${NC}"
        return
    fi

    run_project_benchmark "vite-vanilla-ts-app" "$tsconfig" "$src_dir"
    echo
}

ensure_large_ts_repo_fixture() {
    # The root tsconfig.json in large-ts-repo uses project references, so
    # `tsc/tsgo/tsz --noEmit -p tsconfig.json` exits almost immediately
    # without type-checking anything. Use a flat tsconfig that directly
    # includes all source files for an apples-to-apples measurement.
    #
    # Prefer tsconfig.flat.bench.json if the repo ships it: it already
    # extends tsconfig.base.json which contains the 200+ `paths` mappings
    # for cross-package @scope/pkg imports. Without those paths, tsc itself
    # emits resolution errors and the benchmark is skipped.
    tsz_ensure_large_ts_repo_fixture "$LARGE_TS_DIR" "$LARGE_TS_REPO" "$LARGE_TS_REF" || return 1
}

run_large_ts_repo_benchmarks() {
    if ! is_benchmark_selected "large-ts-repo"; then
        return
    fi

    print_header "Real-world External Project - large-ts-repo (6000+ files, parallel stress test)"
    ensure_large_ts_repo_fixture || return $?
    echo -e "${GREEN}✓${NC} large-ts-repo pinned at $(git -C "$LARGE_TS_DIR" rev-parse --short HEAD)"

    # Use the flat tsconfig so all source files are included in a single
    # compilation pass. The root tsconfig.json uses project references and
    # completes in milliseconds without actually checking any files.
    # Prefer tsconfig.flat.bench.json (ships with the repo, extends base
    # with full path mappings) over our generated tsconfig.flat.json.
    local tsconfig
    if ! tsconfig="$(tsz_large_ts_repo_select_tsconfig "$LARGE_TS_DIR")"; then
        echo -e "${RED}✗ No flat tsconfig found (ensure_large_ts_repo_fixture should have created one)${NC}"
        return
    fi
    local src_dir="$LARGE_TS_DIR/packages"

    run_project_benchmark "large-ts-repo" "$tsconfig" "$src_dir"
    echo
}

cleanup_benchmark_temp() {
    local exit_code=$?
    export_results_json || true
    rm -rf -- "${TEMP_DIR:?}"
    return "$exit_code"
}

main() {
    check_prerequisites
    if [ "$PREPARE_ONLY" = true ]; then
        echo -e "${GREEN}Benchmark prerequisites ready.${NC}"
        return
    fi

    # Create temp directory for synthetic files
    TEMP_DIR=$(mktemp -d)
    PROJECT_COMPATIBILITY_JSONL="$TEMP_DIR/project-compatibility.jsonl"
    BENCHMARK_SOURCES_JSONL="$TEMP_DIR/benchmark-sources.jsonl"
    : > "$PROJECT_COMPATIBILITY_JSONL"
    : > "$BENCHMARK_SOURCES_JSONL"
    # Always export the partial JSON on exit (including SIGTERM/SIGINT/OOM
    # kills) so a long bench that gets cut off — e.g. by the GitHub Actions
    # job timeout or the runner OOM killer on `large-ts-repo` — still
    # surfaces the rows that DID complete. Without this, an exit at any
    # point past the first benchmark would lose the entire dataset, leaving
    # the gh-pages deploy with no fresh artifact.
    trap cleanup_benchmark_temp EXIT
    trap "exit 130" INT
    trap "exit 143" TERM

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

    run_isolated "utility-types"          run_utility_types_benchmarks
    run_isolated "ts-toolbelt"            run_ts_toolbelt_benchmarks
    run_isolated "ts-essentials"          run_ts_essentials_benchmarks
    run_isolated "utility-types-project"  run_utility_types_project_benchmarks
    run_isolated "ts-toolbelt-project"    run_ts_toolbelt_project_benchmarks
    run_isolated "ts-essentials-project"  run_ts_essentials_project_benchmarks
    run_isolated "rxjs-project"           run_rxjs_project_benchmarks
    run_isolated "type-fest-project"      run_type_fest_project_benchmarks
    run_isolated "zod-project"            run_zod_project_benchmarks
    run_isolated "kysely-project"         run_kysely_project_benchmarks
    run_isolated "valibot-project"        run_valibot_project_benchmarks
    run_isolated "msw-project"            run_msw_project_benchmarks
    run_isolated "comlink-project"        run_comlink_project_benchmarks
    run_isolated "effect-project"         run_effect_project_benchmarks
    run_isolated "drizzle-orm-project"    run_drizzle_orm_project_benchmarks
    run_isolated "ts-rest-project"        run_ts_rest_project_benchmarks
    run_isolated "ofetch-project"         run_ofetch_project_benchmarks
    run_isolated "ts-pattern-project"     run_ts_pattern_project_benchmarks
    run_isolated "radash-project"                    run_radash_project_benchmarks
    run_isolated "valtio-project"                    run_valtio_project_benchmarks
    run_isolated "scule-project"                     run_scule_project_benchmarks
    run_isolated "mitt-project"                      run_mitt_project_benchmarks
    run_isolated "change-case-project"               run_change_case_project_benchmarks
    run_isolated "tiny-invariant-project"            run_tiny_invariant_project_benchmarks
    run_isolated "ts-belt-project"                   run_ts_belt_project_benchmarks
    run_isolated "ts-extras-project"                 run_ts_extras_project_benchmarks
    run_isolated "superjson-project"                 run_superjson_project_benchmarks
    run_isolated "trpc-project"                      run_trpc_project_benchmarks
    run_isolated "tanstack-query-project"            run_tanstack_query_project_benchmarks
    run_isolated "tanstack-router-project"           run_tanstack_router_project_benchmarks
    run_isolated "zustand-project"                   run_zustand_project_benchmarks
    run_isolated "jotai-project"                     run_jotai_project_benchmarks
    run_isolated "fp-ts-project"                     run_fp_ts_project_benchmarks
    run_isolated "io-ts-project"                     run_io_ts_project_benchmarks
    run_isolated "immer-project"                     run_immer_project_benchmarks
    run_isolated "remeda-project"                    run_remeda_project_benchmarks
    run_isolated "ts-morph-project"                  run_ts_morph_project_benchmarks
    run_isolated "arktype-project"                   run_arktype_project_benchmarks
    run_isolated "superstruct-project"               run_superstruct_project_benchmarks
    run_isolated "runtypes-project"                  run_runtypes_project_benchmarks
    run_isolated "hotscript-project"                 run_hotscript_project_benchmarks
    run_isolated "typebox-project"                   run_typebox_project_benchmarks
    run_isolated "class-transformer-project"         run_class_transformer_project_benchmarks
    run_isolated "type-graphql-project"              run_type_graphql_project_benchmarks
    run_isolated "neverthrow-project"                run_neverthrow_project_benchmarks
    run_isolated "xstate-project"                    run_xstate_project_benchmarks
    run_isolated "mobx-project"                      run_mobx_project_benchmarks
    run_isolated "umami-project"                     run_application_project_benchmarks "umami-project"
    run_isolated "excalidraw-project"                run_application_project_benchmarks "excalidraw-project"
    run_isolated "dub-project"                       run_application_project_benchmarks "dub-project"
    run_isolated "formbricks-project"                run_application_project_benchmarks "formbricks-project"
    run_isolated "typebot-project"                   run_application_project_benchmarks "typebot-project"
    run_isolated "lobe-chat-project"                 run_application_project_benchmarks "lobe-chat-project"
    run_isolated "supabase-studio-project"           run_application_project_benchmarks "supabase-studio-project"
    # infisical-project is perf_timed:false (its vs-tsgo perf benchmark errors).
    # Compatibility is still tracked via the compile-canary (run_application_row),
    # so it is intentionally absent from the perf runner. See project-rows.mjs.
    run_isolated "payload-project"                   run_application_project_benchmarks "payload-project"
    run_isolated "medusa-project"                    run_application_project_benchmarks "medusa-project"
    run_isolated "outline-project"                   run_application_project_benchmarks "outline-project"
    run_isolated "trigger-dev-project"               run_application_project_benchmarks "trigger-dev-project"
    run_isolated "joplin-project"                    run_application_project_benchmarks "joplin-project"
    run_isolated "directus-project"                  run_application_project_benchmarks "directus-project"
    run_isolated "n8n-project"                       run_application_project_benchmarks "n8n-project"
    run_isolated "cal-com-project"                   run_application_project_benchmarks "cal-com-project"
    run_isolated "documenso-project"                 run_application_project_benchmarks "documenso-project"
    run_isolated "affine-project"                    run_application_project_benchmarks "affine-project"
    run_isolated "immich-server-project"             run_application_project_benchmarks "immich-server-project"
    run_isolated "rocketchat-project"                run_application_project_benchmarks "rocketchat-project"
    run_isolated "vite-vanilla-ts-app"    run_vite_app_project_benchmarks
    run_isolated "nextjs-fresh-app"       run_next_app_project_benchmarks
    run_isolated "nextjs"                 run_nextjs_benchmarks
    run_isolated "large-ts-repo"          run_large_ts_repo_benchmarks

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

        file="$TEMP_DIR/recursive_utility_alias_30.ts"
        generate_recursive_utility_alias_file 30 "$file"
        run_benchmark "Recursive utility aliases N=30" "$file"
        echo

        file="$TEMP_DIR/shallow_optional_50.ts"
        generate_shallow_optional_chain_file 50 "$file"
        run_benchmark "Shallow optional-chain N=50" "$file"
        echo

        file="$TEMP_DIR/indexed_access_hotspot_25.ts"
        generate_indexed_access_hotspot_file 25 "$file"
        run_benchmark "Indexed access hotspot N=25" "$file"
        echo

        file="$TEMP_DIR/remapped_accessor_hotspot_25.ts"
        generate_remapped_accessor_hotspot_file 25 "$file"
        run_benchmark "Remapped accessor hotspot N=25" "$file"
        echo

        file="$TEMP_DIR/conditional_infer_hotspot_25.ts"
        generate_conditional_infer_hotspot_file 25 "$file"
        run_benchmark "Conditional infer hotspot N=25" "$file"
        echo

        file="$TEMP_DIR/object_spread_hotspot_25.ts"
        generate_object_spread_hotspot_file 25 "$file"
        run_benchmark "Object spread hotspot N=25" "$file"
        echo

        file="$TEMP_DIR/contextual_callback_hotspot_25.ts"
        generate_contextual_callback_hotspot_file 25 "$file"
        run_benchmark "Contextual callback hotspot N=25" "$file"
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

        for count in 120 240; do
            file="$TEMP_DIR/recursive_utility_alias_${count}.ts"
            generate_recursive_utility_alias_file "$count" "$file"
            run_benchmark "Recursive utility aliases N=$count" "$file"
            echo
        done

        file="$TEMP_DIR/shallow_optional_400.ts"
        generate_shallow_optional_chain_file 400 "$file"
        run_benchmark "Shallow optional-chain N=400" "$file"
        echo

        print_subheader "Project hotspot microbenchmarks"

        for count in 25 50 100 200; do
            local file="$TEMP_DIR/indexed_access_hotspot_${count}.ts"
            generate_indexed_access_hotspot_file "$count" "$file"
            run_benchmark "Indexed access hotspot N=$count" "$file"
            echo
        done

        for count in 25 50 100 200; do
            local file="$TEMP_DIR/remapped_accessor_hotspot_${count}.ts"
            generate_remapped_accessor_hotspot_file "$count" "$file"
            run_benchmark "Remapped accessor hotspot N=$count" "$file"
            echo
        done

        for count in 25 50 100 200; do
            local file="$TEMP_DIR/conditional_infer_hotspot_${count}.ts"
            generate_conditional_infer_hotspot_file "$count" "$file"
            run_benchmark "Conditional infer hotspot N=$count" "$file"
            echo
        done

        for count in 25 50 100 200; do
            local file="$TEMP_DIR/object_spread_hotspot_${count}.ts"
            generate_object_spread_hotspot_file "$count" "$file"
            run_benchmark "Object spread hotspot N=$count" "$file"
            echo
        done

        for count in 25 50 100 200; do
            local file="$TEMP_DIR/contextual_callback_hotspot_${count}.ts"
            generate_contextual_callback_hotspot_file "$count" "$file"
            run_benchmark "Contextual callback hotspot N=$count" "$file"
            echo
        done
        
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
        echo "  ./scripts/bench/bench-vs-tsgo.sh --quick --filter 'utility-types'"
        echo "  ./scripts/bench/bench-vs-tsgo.sh --quick --filter 'BCT|CFA'"
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
