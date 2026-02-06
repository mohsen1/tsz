#!/usr/bin/env bash
#
# Microbenchmark: tsz vs tsgo on largeControlFlowGraph.ts only
#
# Fast iteration workflow:
#   1) Build once with --rebuild
#   2) Re-run this script without rebuilding while tuning code
#
# Usage:
#   ./scripts/bench-large-control-flow-micro.sh
#   ./scripts/bench-large-control-flow-micro.sh --rebuild
#   ./scripts/bench-large-control-flow-micro.sh --min-runs 3 --max-runs 6
#   ./scripts/bench-large-control-flow-micro.sh --json /tmp/lcfg.json

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

BENCH_TARGET_DIR="${BENCH_TARGET_DIR:-$PROJECT_ROOT/.target-bench}"
TSZ="$BENCH_TARGET_DIR/dist/tsz"
TSGO="${TSGO:-$(command -v tsgo 2>/dev/null || true)}"
TEST_FILE="$PROJECT_ROOT/TypeScript/tests/cases/compiler/largeControlFlowGraph.ts"

FORCE_REBUILD=false
WARMUP=1
MIN_RUNS=2
MAX_RUNS=4
JSON_OUT=""

usage() {
    cat << 'EOF'
Usage: ./scripts/bench-large-control-flow-micro.sh [OPTIONS]

Options:
  --rebuild          Force rebuild tsz binary using dist profile
  --warmup N         Hyperfine warmup runs (default: 1)
  --min-runs N       Hyperfine minimum runs (default: 2)
  --max-runs N       Hyperfine maximum runs (default: 4)
  --json PATH        Save hyperfine JSON output to PATH
  --help, -h         Show this help

Notes:
  - This benchmark targets only TypeScript/tests/cases/compiler/largeControlFlowGraph.ts.
  - Binary reuse is intentional for fast iteration; use --rebuild after code changes.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --rebuild)
            FORCE_REBUILD=true
            shift
            ;;
        --warmup)
            WARMUP="${2:?missing value for --warmup}"
            shift 2
            ;;
        --min-runs)
            MIN_RUNS="${2:?missing value for --min-runs}"
            shift 2
            ;;
        --max-runs)
            MAX_RUNS="${2:?missing value for --max-runs}"
            shift 2
            ;;
        --json)
            JSON_OUT="${2:?missing value for --json}"
            shift 2
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

if ! command -v hyperfine >/dev/null 2>&1; then
    echo "hyperfine is required. Install with: brew install hyperfine" >&2
    exit 1
fi

if [[ -z "$TSGO" || ! -x "$TSGO" ]]; then
    echo "tsgo not found. Install with: npm install -g @typescript/native-preview" >&2
    exit 1
fi

if [[ ! -f "$TEST_FILE" ]]; then
    echo "Benchmark file not found: $TEST_FILE" >&2
    exit 1
fi

if [[ "$FORCE_REBUILD" == true || ! -x "$TSZ" ]]; then
    echo "Building tsz (dist profile) into $BENCH_TARGET_DIR ..."
    (cd "$PROJECT_ROOT" && CARGO_TARGET_DIR="$BENCH_TARGET_DIR" cargo build --profile dist --features cli)
fi

if [[ ! -x "$TSZ" ]]; then
    echo "tsz binary not found after build: $TSZ" >&2
    exit 1
fi

tsz_cmd="$(printf '\"%s\" --noEmit \"%s\" 2>/dev/null' "$TSZ" "$TEST_FILE")"
tsgo_cmd="$(printf '\"%s\" --noEmit \"%s\" 2>/dev/null' "$TSGO" "$TEST_FILE")"

json_file="$JSON_OUT"
cleanup_json=false
if [[ -z "$json_file" ]]; then
    json_file="$(mktemp)"
    cleanup_json=true
fi

echo "Microbenchmark target: $(basename "$TEST_FILE")"
echo "tsz:  $TSZ"
echo "tsgo: $TSGO"
echo "runs: warmup=$WARMUP min=$MIN_RUNS max=$MAX_RUNS"
echo

hyperfine \
    --warmup "$WARMUP" \
    --min-runs "$MIN_RUNS" \
    --max-runs "$MAX_RUNS" \
    --style full \
    --ignore-failure \
    --export-json "$json_file" \
    -n "tsz" "$tsz_cmd" \
    -n "tsgo" "$tsgo_cmd"

if command -v jq >/dev/null 2>&1; then
    tsz_mean="$(jq -r '.results[] | select(.command | contains("tsz")) | .mean' "$json_file")"
    tsgo_mean="$(jq -r '.results[] | select(.command | contains("tsgo")) | .mean' "$json_file")"
    tsz_ms="$(printf "%.2f" "$(echo "$tsz_mean * 1000" | bc -l)")"
    tsgo_ms="$(printf "%.2f" "$(echo "$tsgo_mean * 1000" | bc -l)")"

    if (( $(echo "$tsz_mean < $tsgo_mean" | bc -l) )); then
        factor="$(printf "%.2f" "$(echo "$tsgo_mean / $tsz_mean" | bc -l)")"
        echo
        echo "Result: tsz wins by ${factor}x (${tsz_ms}ms vs ${tsgo_ms}ms)"
    else
        factor="$(printf "%.2f" "$(echo "$tsz_mean / $tsgo_mean" | bc -l)")"
        echo
        echo "Result: tsgo wins by ${factor}x (${tsz_ms}ms vs ${tsgo_ms}ms)"
    fi
fi

if [[ "$cleanup_json" == true ]]; then
    rm -f "$json_file"
fi
