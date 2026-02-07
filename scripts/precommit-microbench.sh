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
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

[[ "${TSZ_SKIP_BENCH:-}" == "1" ]] && exit 0

UPDATE_BASELINE=false
NO_BUILD=false

usage() {
    cat <<'EOF'
Usage: ./scripts/precommit-microbench.sh [OPTIONS]

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

generate_bct_stress_file() {
    local count="$1"
    local output="$2"

    cat > "$output" << 'HEADER'
// Pre-commit BCT microbenchmark
class Base { base: string = ''; }
HEADER

    for ((i=0; i<count; i++)); do
        echo "class Derived$i extends Base { prop$i: number = $i; }" >> "$output"
    done
    echo "" >> "$output"

    echo -n "const items = [" >> "$output"
    for ((i=0; i<count; i++)); do
        if [ "$i" -gt 0 ]; then echo -n ", " >> "$output"; fi
        echo -n "new Derived$i()" >> "$output"
    done
    echo "];" >> "$output"
    echo "" >> "$output"

    echo "function pickOne(index: number) {" >> "$output"
    for ((i=0; i<count; i++)); do
        echo "  if (index === $i) return new Derived$i();" >> "$output"
    done
    echo "  return new Base();" >> "$output"
    echo "}" >> "$output"
    echo "" >> "$output"

    echo "function identity<T>(x: T): T { return x; }" >> "$output"
    echo -n "const mixed = [" >> "$output"
    for ((i=0; i<count; i++)); do
        if [ "$i" -gt 0 ]; then echo -n ", " >> "$output"; fi
        echo -n "identity(new Derived$i())" >> "$output"
    done
    echo "];" >> "$output"
    echo "" >> "$output"

    echo "declare const flag: number;" >> "$output"
    echo -n "const chosen = " >> "$output"
    for ((i=0; i<count; i++)); do
        echo -n "flag === $i ? new Derived$i() : " >> "$output"
    done
    echo "new Base();" >> "$output"
    echo "" >> "$output"

    echo "const _base: Base = items[0];" >> "$output"
    echo "const _picked: Base = pickOne(0);" >> "$output"
    echo "const _chosen: Base = chosen;" >> "$output"
}

generate_constraint_conflict_file() {
    local count="$1"
    local output="$2"

    cat > "$output" << 'HEADER'
// Pre-commit constraint conflict microbenchmark
HEADER

    for ((i=0; i<count; i++)); do
        echo "interface Constraint$i { key$i: string; shared: number; }" >> "$output"
    done
    echo "" >> "$output"

    for ((i=0; i<count; i++)); do
        echo "declare function constrain$i<T extends Constraint$i>(x: T): T;" >> "$output"
    done
    echo "" >> "$output"

    for ((i=0; i<count; i++)); do
        echo -n "const obj$i = { shared: $i" >> "$output"
        for ((j=0; j<=i && j<count; j++)); do
            echo -n ", key$j: 'val'" >> "$output"
        done
        echo " };" >> "$output"
    done
    echo "" >> "$output"

    for ((i=0; i<count; i++)); do
        echo "const res$i = constrain$i(obj$i);" >> "$output"
    done
    echo "" >> "$output"

    echo -n "function multiConstrained<T extends " >> "$output"
    for ((i=0; i<count; i++)); do
        if [ "$i" -gt 0 ]; then echo -n " & " >> "$output"; fi
        echo -n "Constraint$i" >> "$output"
    done
    echo ">(x: T): T { return x; }" >> "$output"
    echo "" >> "$output"

    echo -n "const allConstraints = { shared: 0" >> "$output"
    for ((i=0; i<count; i++)); do
        echo -n ", key$i: 'val'" >> "$output"
    done
    echo " };" >> "$output"
    echo "const _result = multiConstrained(allConstraints);" >> "$output"
}

temp_dir="$(mktemp -d)"
trap 'rm -rf "$temp_dir"' EXIT

large_cfa="$ROOT_DIR/TypeScript/tests/cases/compiler/largeControlFlowGraph.ts"
bct_case="$temp_dir/bct_100.ts"
constraint_case="$temp_dir/constraint_conflicts_100.ts"

generate_bct_stress_file 100 "$bct_case"
generate_constraint_conflict_file 100 "$constraint_case"

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

node "$SCRIPT_DIR/precommit-microbench.mjs" \
    --bin "$TSZ_BIN" \
    --baseline "$BASELINE_FILE" \
    --threshold-pct "$THRESHOLD_PCT" \
    --runs "$RUNS" \
    --warmup "$WARMUP" \
    --update-baseline "$update_flag" \
    --profile "$PROFILE" \
    --case "largeControlFlowGraph=$large_cfa" \
    --case "bct_candidates_100=$bct_case" \
    --case "constraint_conflicts_100=$constraint_case"
