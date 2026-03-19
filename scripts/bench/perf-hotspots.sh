#!/usr/bin/env bash
#
# Targeted hotspot performance suite.
#
# Focuses on known scaling hotspots so perf iterations can be measured quickly
# and consistently while preserving compatibility with bench-vs-tsgo output.
#
# Usage:
#   ./scripts/bench/perf-hotspots.sh
#   ./scripts/bench/perf-hotspots.sh --quick
#   ./scripts/bench/perf-hotspots.sh --rebuild
#   ./scripts/bench/perf-hotspots.sh --json-file artifacts/perf/hotspots-baseline.json
#   ./scripts/bench/perf-hotspots.sh --filter 'BCT candidates|Constraint conflicts'

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BENCH_SCRIPT="$PROJECT_ROOT/scripts/bench/bench-vs-tsgo.sh"

# Focus on the current requested losers.
# In --quick mode bench-vs-tsgo only emits reduced-size representatives for the
# same hotspot families, so the default filter must track those quick labels.
DEFAULT_FILTER_FULL='^(DeepPartial optional-chain N=400|Shallow optional-chain N=400|200 union members|200 generic functions|Constraint conflicts N=200|200 classes)$'
DEFAULT_FILTER_QUICK='^(DeepPartial optional-chain N=50|Shallow optional-chain N=50|50 generic functions|100 classes|Constraint conflicts N=30)$'
FILTER=""
JSON_FILE="$PROJECT_ROOT/artifacts/perf/hotspots-$(date +%Y%m%d-%H%M%S).json"
QUICK_MODE=false
FORCE_REBUILD=false
EXTRA_ARGS=()

usage() {
    cat <<'USAGE'
Usage: ./scripts/bench/perf-hotspots.sh [OPTIONS] [-- <extra bench-vs-tsgo args>]

Options:
  --quick            Use quick mode for faster iteration
  --rebuild          Force rebuild of tsz benchmark binary
  --filter REGEX     Override hotspot filter regex
  --json-file PATH   JSON output file path
  --help             Show this help

Notes:
  - This script delegates execution to scripts/bench/bench-vs-tsgo.sh.
  - JSON output is always enabled.
USAGE
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --quick)
            QUICK_MODE=true
            shift
            ;;
        --rebuild)
            FORCE_REBUILD=true
            shift
            ;;
        --filter)
            FILTER="$2"
            shift 2
            ;;
        --json-file)
            JSON_FILE="$2"
            shift 2
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        --)
            shift
            while [[ $# -gt 0 ]]; do
                EXTRA_ARGS+=("$1")
                shift
            done
            ;;
        *)
            EXTRA_ARGS+=("$1")
            shift
            ;;
    esac
done

if [[ -z "$FILTER" ]]; then
    if [[ "$QUICK_MODE" == true ]]; then
        FILTER="$DEFAULT_FILTER_QUICK"
    else
        FILTER="$DEFAULT_FILTER_FULL"
    fi
fi

if [[ ! -x "$BENCH_SCRIPT" ]]; then
    echo "Benchmark script not found or not executable: $BENCH_SCRIPT" >&2
    exit 1
fi

mkdir -p "$(dirname "$JSON_FILE")"

CMD=("$BENCH_SCRIPT" "--filter" "$FILTER" "--json-file" "$JSON_FILE")
if [[ "$QUICK_MODE" == true ]]; then
    CMD+=("--quick")
fi
if [[ "$FORCE_REBUILD" == true ]]; then
    CMD+=("--rebuild")
fi
if [[ ${#EXTRA_ARGS[@]} -gt 0 ]]; then
    CMD+=("${EXTRA_ARGS[@]}")
fi

echo "Running hotspot suite with filter: $FILTER"
echo "JSON output: $JSON_FILE"
"${CMD[@]}"
