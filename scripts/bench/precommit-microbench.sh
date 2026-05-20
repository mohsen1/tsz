#!/usr/bin/env bash
#
# Fast microbenchmark regression gate for pre-commit.
#
# Goals:
# - Catch major performance regressions before commit.
# - Keep runtime low enough for frequent local use.
# - Avoid machine-to-machine noise by storing baseline locally in .git/.
#
# Environment variables:
#   TSZ_SKIP_BENCH=1                Skip benchmark checks entirely.
#   TSZ_BENCH_PROFILE=dev           Cargo profile used to build tsz.
#   TSZ_BENCH_TARGET_DIR=target     Cargo target dir.
#   TSZ_BENCH_RUNS=4                Timed runs per case.
#   TSZ_BENCH_WARMUP=1              Warmup runs per case.
#   TSZ_BENCH_THRESHOLD_PCT=12      Allowed regression per case (%).
#   TSZ_BENCH_BASELINE_FILE=...     Baseline JSON path.
#
# Flags:
#   --update-baseline               Replace baseline with current measurements.
#   --no-build                      Skip cargo build (use existing binary).
#   --help                          Show help.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT_DIR"

[[ "${TSZ_SKIP_BENCH:-}" == "1" ]] && exit 0

UPDATE_BASELINE=false
NO_BUILD=false

usage() {
    cat <<'EOF'
Usage: ./scripts/bench/precommit-microbench.sh [OPTIONS]

Options:
  --update-baseline   Replace baseline with current measurements
  --no-build          Skip cargo build and use existing tsz binary
  --help              Show this help

Environment:
  TSZ_SKIP_BENCH=1
  TSZ_BENCH_PROFILE=dev
  TSZ_BENCH_TARGET_DIR=target
  TSZ_BENCH_RUNS=4
  TSZ_BENCH_WARMUP=1
  TSZ_BENCH_THRESHOLD_PCT=12
  TSZ_BENCH_BASELINE_FILE=.git/tsz-microbench-baseline.json
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --update-baseline)
            UPDATE_BASELINE=true
            shift
            ;;
        --no-build)
            NO_BUILD=true
            shift
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage
            exit 2
            ;;
    esac
done

if ! command -v cargo >/dev/null 2>&1; then
    echo "❌ cargo not found"
    exit 1
fi

if ! command -v node >/dev/null 2>&1; then
    echo "❌ node not found (required for timing runner)"
    exit 1
fi

PROFILE="${TSZ_BENCH_PROFILE:-dev}"
TARGET_DIR="${TSZ_BENCH_TARGET_DIR:-$ROOT_DIR/target}"
RUNS="${TSZ_BENCH_RUNS:-4}"
WARMUP="${TSZ_BENCH_WARMUP:-1}"
THRESHOLD_PCT="${TSZ_BENCH_THRESHOLD_PCT:-12}"
BASELINE_FILE="${TSZ_BENCH_BASELINE_FILE:-$ROOT_DIR/.git/tsz-microbench-baseline.json}"

profile_dir="$PROFILE"
if [[ "$PROFILE" == "dev" ]]; then
    profile_dir="debug"
elif [[ "$PROFILE" == "release" ]]; then
    profile_dir="release"
fi
TSZ_BIN="$TARGET_DIR/$profile_dir/tsz"

# Fixture generators are shared with bench-vs-tsgo.sh. One source of truth
# keeps the regression gate aligned with the larger benchmark suite, so a case
# that locks in a hotspot here uses the same shape that bench-vs-tsgo measures.
# shellcheck source=lib/synthetic-generators.sh
source "$SCRIPT_DIR/lib/synthetic-generators.sh"

temp_dir="$(mktemp -d)"
trap 'rm -rf "$temp_dir"' EXIT

# Each microbench case targets a distinct perf hotspot that has surfaced in
# project benchmarks. Sizes are small enough for fast local iteration but
# large enough that a real algorithmic regression cannot hide inside noise.
declare -a MICROBENCH_CASE_NAMES=()
declare -a MICROBENCH_CASE_FILES=()

# Add a case backed by an existing fixture file (e.g. a TypeScript submodule
# sample). Used for fixtures that aren't shell-generated.
register_case() {
    MICROBENCH_CASE_NAMES+=("$1")
    MICROBENCH_CASE_FILES+=("$2")
}

# Generate a fixture via one of the shared generators and register it. The
# generator is invoked with any sizing arguments followed by the output path,
# which matches the generator function signature in lib/synthetic-generators.sh.
register_generated() {
    local case_name="$1"
    local generator="$2"
    shift 2
    local path="$temp_dir/$case_name.ts"
    "$generator" "$@" "$path"
    register_case "$case_name" "$path"
}

# largeControlFlowGraph is the only non-generated case; it comes from the
# TypeScript submodule. Keep it first so its baseline entry name is stable.
register_case "largeControlFlowGraph" \
    "$ROOT_DIR/TypeScript/tests/cases/compiler/largeControlFlowGraph.ts"

# Generated synthetic cases. Each line: register_generated <case-name> <generator> [size].
# The case name is also the basename of the .ts file under $temp_dir.
register_generated "bct_candidates_100"            generate_bct_stress_file                  100
register_generated "constraint_conflicts_100"      generate_constraint_conflict_file         100
register_generated "classes_30"                    generate_synthetic_file                    30
register_generated "complex_generics_25"           generate_complex_file                      25
# DeepPartial+optional chain triggers recursive mapped-type evaluation per
# property access; small N is enough to lock in regressions while keeping
# per-case runtime under the local-iteration budget.
register_generated "deeppartial_optional_chain_15" generate_deeppartial_optional_chain_file   15
register_generated "shallow_optional_chain_15"     generate_shallow_optional_chain_file       15
register_generated "typed_arrays"                  generate_typed_arrays_file
register_generated "union_members_50"              generate_union_file                        50
# Larger union variant exercises the algorithmic cliff that the 50-member
# variant cannot reach; both are kept because a regression often appears only
# at one of the two scales.
register_generated "union_members_100"             generate_union_file                       100
register_generated "recursive_generic_depth_20"    generate_recursive_generic_file            20
register_generated "conditional_distribution_40"   generate_conditional_distribution_file     40
register_generated "mapped_type_keys_80"           generate_mapped_type_file                  80
# Larger mapped-keys variant probes the MAX_MAPPED_KEYS scaling regime;
# kept just under the runtime budget because each application iterates every
# property through a non-trivial homomorphic body.
register_generated "mapped_type_keys_150"          generate_mapped_type_file                 150
register_generated "template_literal_20"           generate_template_literal_file             20
register_generated "deep_subtype_depth_25"         generate_deep_subtype_file                 25
register_generated "intersection_25"               generate_intersection_file                 25
register_generated "infer_stress_15"               generate_infer_stress_file                 15
register_generated "cfa_branches_40"               generate_cfa_stress_file                   40
# Complex-template mapped types apply 6+ non-trivial mapped types over each
# property (FormField recurses via nested conditionals); per-case cost is
# the highest in the gate, so size is kept low.
register_generated "mapped_complex_template_15"    generate_mapped_complex_template_file      15
register_generated "keyof_chain_20"                generate_keyof_chain_file                  20
register_generated "overload_resolution_25"        generate_overload_resolution_file          25
register_generated "object_literal_assign_20"      generate_object_literal_assign_file        20

if [[ "$NO_BUILD" != true ]]; then
    echo "   Building benchmark binary (profile=$PROFILE)..."
    CARGO_TARGET_DIR="$TARGET_DIR" cargo build --quiet --profile "$PROFILE" -p tsz-cli --bin tsz
fi

if [[ ! -x "$TSZ_BIN" ]]; then
    echo "❌ tsz binary not found: $TSZ_BIN"
    exit 1
fi

update_flag="0"
if [[ "$UPDATE_BASELINE" == true ]]; then
    update_flag="1"
fi

case_args=()
for idx in "${!MICROBENCH_CASE_NAMES[@]}"; do
    case_args+=(--case "${MICROBENCH_CASE_NAMES[$idx]}=${MICROBENCH_CASE_FILES[$idx]}")
done

node "$SCRIPT_DIR/precommit-microbench.mjs" \
    --bin "$TSZ_BIN" \
    --baseline "$BASELINE_FILE" \
    --threshold-pct "$THRESHOLD_PCT" \
    --runs "$RUNS" \
    --warmup "$WARMUP" \
    --update-baseline "$update_flag" \
    --profile "$PROFILE" \
    "${case_args[@]}"
