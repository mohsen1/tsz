#!/usr/bin/env bash
#
# Targeted hotspot performance suite.
#
# Focuses on known scaling hotspots so perf iterations can be measured quickly
# and consistently while preserving compatibility with bench-vs-tsgo output.
#
# Usage:
#   ./scripts/perf-hotspots.sh
#   ./scripts/perf-hotspots.sh --quick
#   ./scripts/perf-hotspots.sh --rebuild
#   ./scripts/perf-hotspots.sh --json-file artifacts/perf/hotspots-baseline.json
#   ./scripts/perf-hotspots.sh --filter 'BCT candidates|Constraint conflicts'

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BENCH_SCRIPT="$PROJECT_ROOT/scripts/bench-vs-tsgo.sh"

DEFAULT_FILTER='^(BCT candidates=50|BCT candidates=100|BCT candidates=200|Constraint conflicts N=50|Constraint conflicts N=100|Constraint conflicts N=200|CFA branches=50|CFA branches=100|CFA branches=150)$'
FILTER="$DEFAULT_FILTER"
JSON_FILE="$PROJECT_ROOT/artifacts/perf/hotspots-$(date +%Y%m%d-%H%M%S).json"
QUICK_MODE=false
FORCE_REBUILD=false
EXTRA_ARGS=()

usage() {
    cat <<'USAGE'
Usage: ./scripts/perf-hotspots.sh [OPTIONS] [-- <extra bench-vs-tsgo args>]

Options:
  --quick            Use quick mode for faster iteration
  --rebuild          Force rebuild of tsz benchmark binary
  --filter REGEX     Override hotspot filter regex
  --json-file PATH   JSON output file path
  --help             Show this help

Notes:
  - This script delegates execution to scripts/bench-vs-tsgo.sh.
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
