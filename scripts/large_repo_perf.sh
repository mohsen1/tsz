#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

ROOT_TSCONFIG_DEFAULT="/Users/mohsen/code/large-ts-repo/tsconfig.json"
SLICE_TSCONFIG_DEFAULT="/Users/mohsen/code/large-ts-repo/packages/domain/recovery-stress-lab/tsconfig.json"

TSZ_LIB_DIR_DEFAULT="$PROJECT_ROOT/src/lib-assets"
TSZ_LIB_DIR="${TSZ_LIB_DIR:-$TSZ_LIB_DIR_DEFAULT}"

PERF_TARGET_DIR="${PERF_TARGET_DIR:-$PROJECT_ROOT/.target-large-repo-perf}"
TSZ_BIN="$PERF_TARGET_DIR/release/tsz"
TSGO_TOOL_DIR="${TSGO_TOOL_DIR:-$PERF_TARGET_DIR/tools/tsgo}"
TSGO_LOCAL_BIN="$TSGO_TOOL_DIR/node_modules/.bin/tsgo"
TSC_TOOL_DIR="${TSC_TOOL_DIR:-$PERF_TARGET_DIR/tools/tsc}"
TSC_LOCAL_BIN="$TSC_TOOL_DIR/node_modules/.bin/tsc"

TSGO_NPM_SPEC="${TSGO_NPM_SPEC:-@typescript/native-preview@7.0.0-dev.20260206.1}"
TSC_NPM_SPEC="${TSC_NPM_SPEC:-}"

RUNS=1
TIMEOUT_SECONDS=180
ADDRESSSPACE_LIMIT="${ADDRESSSPACE_LIMIT:-6g}"
DATASIZE_LIMIT="${DATASIZE_LIMIT:-6g}"
NODE_HEAP_MB="${NODE_HEAP_MB:-5632}"
TOOLS_CSV="tsz,tsc,tsgo"
TARGETS_CSV="root,slice"
ROOT_TSCONFIG="$ROOT_TSCONFIG_DEFAULT"
SLICE_TSCONFIG="$SLICE_TSCONFIG_DEFAULT"
FORCE_REBUILD=false
OUTPUT_DIR=""

usage() {
    cat <<EOF
Usage: ./scripts/large_repo_perf.sh [OPTIONS]

Cold-process large-repo benchmark harness for tsz, tsc, and tsgo.

Options:
  --runs N                Number of cold runs per tool/target (default: $RUNS)
  --timeout-seconds N     Alarm timeout per sample (default: $TIMEOUT_SECONDS)
  --tools CSV             Comma-separated tools: tsz,tsc,tsgo
  --targets CSV           Comma-separated targets: root,slice
  --root-tsconfig PATH    Override root monorepo tsconfig
  --slice-tsconfig PATH   Override representative slice tsconfig
  --output-dir PATH       Write artifacts to a specific directory
  --rebuild               Force rebuild of the release tsz binary
  --help                  Show this help

Environment:
  TSZ_LIB_DIR             Override tsz lib assets (default: $TSZ_LIB_DIR_DEFAULT)
  PERF_TARGET_DIR         Isolated target/tool directory (default: $PERF_TARGET_DIR)
  TSGO_NPM_SPEC           Override pinned tsgo npm package
  TSC_NPM_SPEC            Override pinned typescript npm version
  ADDRESSSPACE_LIMIT      zsh address space limit (default: $ADDRESSSPACE_LIMIT)
  DATASIZE_LIMIT          zsh data size limit (default: $DATASIZE_LIMIT)
  NODE_HEAP_MB            Node heap cap for tsc/tsgo (default: $NODE_HEAP_MB)
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --runs)
            RUNS="$2"
            shift 2
            ;;
        --timeout-seconds)
            TIMEOUT_SECONDS="$2"
            shift 2
            ;;
        --tools)
            TOOLS_CSV="$2"
            shift 2
            ;;
        --targets)
            TARGETS_CSV="$2"
            shift 2
            ;;
        --root-tsconfig)
            ROOT_TSCONFIG="$2"
            shift 2
            ;;
        --slice-tsconfig)
            SLICE_TSCONFIG="$2"
            shift 2
            ;;
        --output-dir)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        --rebuild)
            FORCE_REBUILD=true
            shift
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo "Unknown argument: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

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
    if [ -n "${TSC:-}" ]; then
        if [ ! -x "$TSC" ]; then
            echo "TSC is set but not executable: $TSC" >&2
            exit 1
        fi
        return
    fi

    if ! command -v npm >/dev/null 2>&1; then
        echo "npm is required to install tsc for benchmarking" >&2
        exit 1
    fi

    local resolved_spec="$TSC_NPM_SPEC"
    if [ -z "$resolved_spec" ]; then
        resolved_spec="$(resolve_tsc_npm_spec)"
    fi
    if [ -z "$resolved_spec" ]; then
        echo "Unable to resolve a pinned typescript npm version" >&2
        exit 1
    fi

    mkdir -p "$TSC_TOOL_DIR"
    local spec_file="$TSC_TOOL_DIR/.tsc-spec"
    local installed_spec=""
    if [ -f "$spec_file" ]; then
        installed_spec="$(cat "$spec_file")"
    fi

    if [ ! -x "$TSC_LOCAL_BIN" ] || [ "$installed_spec" != "$resolved_spec" ]; then
        echo "Installing tsc locally (typescript@$resolved_spec)..."
        npm install \
            --prefix "$TSC_TOOL_DIR" \
            --no-audit \
            --no-fund \
            --loglevel=error \
            "typescript@$resolved_spec" >/dev/null
        printf '%s\n' "$resolved_spec" > "$spec_file"
    fi

    if [ ! -x "$TSC_LOCAL_BIN" ]; then
        echo "tsc install failed: $TSC_LOCAL_BIN not found" >&2
        exit 1
    fi

    TSC="$TSC_LOCAL_BIN"
}

ensure_tsgo() {
    if [ -n "${TSGO:-}" ]; then
        if [ ! -x "$TSGO" ]; then
            echo "TSGO is set but not executable: $TSGO" >&2
            exit 1
        fi
        return
    fi

    if ! command -v npm >/dev/null 2>&1; then
        echo "npm is required to install tsgo for benchmarking" >&2
        exit 1
    fi

    mkdir -p "$TSGO_TOOL_DIR"
    local spec_file="$TSGO_TOOL_DIR/.tsgo-spec"
    local installed_spec=""
    if [ -f "$spec_file" ]; then
        installed_spec="$(cat "$spec_file")"
    fi

    if [ ! -x "$TSGO_LOCAL_BIN" ] || [ "$installed_spec" != "$TSGO_NPM_SPEC" ]; then
        echo "Installing tsgo locally ($TSGO_NPM_SPEC)..."
        npm install \
            --prefix "$TSGO_TOOL_DIR" \
            --no-audit \
            --no-fund \
            --loglevel=error \
            "$TSGO_NPM_SPEC" >/dev/null
        printf '%s\n' "$TSGO_NPM_SPEC" > "$spec_file"
    fi

    if [ ! -x "$TSGO_LOCAL_BIN" ]; then
        echo "tsgo install failed: $TSGO_LOCAL_BIN not found" >&2
        exit 1
    fi

    TSGO="$TSGO_LOCAL_BIN"
}

ensure_tsz() {
    if [ "$FORCE_REBUILD" = true ] || [ ! -x "$TSZ_BIN" ]; then
        echo "Building release tsz binary into $PERF_TARGET_DIR..."
        env CARGO_BUILD_JOBS=1 cargo build \
            --release \
            --target-dir "$PERF_TARGET_DIR" \
            -p tsz-cli \
            --bin tsz
    fi

    if [ ! -x "$TSZ_BIN" ]; then
        echo "tsz release binary not found at $TSZ_BIN" >&2
        exit 1
    fi
}

tool_version_line() {
    local bin="$1"
    if [ ! -x "$bin" ]; then
        echo "unavailable"
        return
    fi

    local version_line=""
    version_line="$("$bin" --version 2>/dev/null | head -n 1 || true)"
    if [ -z "$version_line" ]; then
        echo "unknown"
    else
        echo "$version_line"
    fi
}

median_node_script() {
    node - "$1" "$2" "$3" "$4" "$5" "$6" "$7" "$8" "$9" "${10}" "${11}" "${12}" "${13}" "${14}" <<'EOF'
const fs = require("fs");
const path = require("path");

const [
  samplesPath,
  reportPath,
  branch,
  commit,
  rootTsconfig,
  sliceTsconfig,
  runs,
  timeoutSeconds,
  toolsCsv,
  targetsCsv,
  addressspaceLimit,
  datasizeLimit,
  nodeHeapMb,
  toolVersionsJson
] = process.argv.slice(2);

const lines = fs.readFileSync(samplesPath, "utf8")
  .split("\n")
  .filter(Boolean);
const samples = lines.map((line) => JSON.parse(line));

function median(values) {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0
    ? (sorted[mid - 1] + sorted[mid]) / 2
    : sorted[mid];
}

const groups = new Map();
for (const sample of samples) {
  const key = `${sample.target}:${sample.tool}`;
  if (!groups.has(key)) groups.set(key, []);
  groups.get(key).push(sample);
}

const summaries = [...groups.entries()].map(([key, group]) => {
  const [target, tool] = key.split(":");
  const completed = group.filter((sample) => sample.completed);
  const durations = completed
    .map((sample) => sample.elapsed_seconds)
    .filter((value) => typeof value === "number");
  const rssValues = completed
    .map((sample) => sample.max_rss_bytes)
    .filter((value) => typeof value === "number");
  const diagValues = completed
    .map((sample) => sample.diagnostic_count)
    .filter((value) => typeof value === "number");

  return {
    target,
    tool,
    runs: group.length,
    completed_runs: completed.length,
    successful_runs: group.filter((sample) => sample.exit_code === 0).length,
    timed_out_runs: group.filter((sample) => sample.timed_out).length,
    median_elapsed_seconds: median(durations),
    median_max_rss_bytes: median(rssValues),
    median_diagnostic_count: median(diagValues)
  };
}).sort((a, b) => {
  if (a.target !== b.target) return a.target.localeCompare(b.target);
  return a.tool.localeCompare(b.tool);
});

const report = {
  benchmark_runner: "scripts/large_repo_perf.sh",
  generated_at: new Date().toISOString(),
  git: { branch, commit },
  settings: {
    runs: Number(runs),
    timeout_seconds: Number(timeoutSeconds),
    tools: toolsCsv.split(",").filter(Boolean),
    targets: targetsCsv.split(",").filter(Boolean),
    root_tsconfig: rootTsconfig,
    slice_tsconfig: sliceTsconfig,
    limits: {
      addressspace: addressspaceLimit,
      datasize: datasizeLimit,
      node_heap_mb: Number(nodeHeapMb)
    }
  },
  tool_versions: JSON.parse(toolVersionsJson),
  samples,
  summaries
};

fs.mkdirSync(path.dirname(reportPath), { recursive: true });
fs.writeFileSync(reportPath, JSON.stringify(report, null, 2));
EOF
}

append_sample() {
    local samples_file="$1"
    local tool="$2"
    local target="$3"
    local run="$4"
    local tsconfig="$5"
    local command="$6"
    local completed="$7"
    local timed_out="$8"
    local exit_code="$9"
    local elapsed="${10}"
    local max_rss="${11}"
    local diag_count="${12}"
    local log_path="${13}"

    node -e '
const fs = require("fs");
const [
  samplesPath,
  tool,
  target,
  run,
  tsconfig,
  command,
  completed,
  timedOut,
  exitCode,
  elapsed,
  maxRss,
  diagCount,
  logPath
] = process.argv.slice(1);
const record = {
  tool,
  target,
  run: Number(run),
  tsconfig,
  command,
  completed: completed === "true",
  timed_out: timedOut === "true",
  exit_code: Number(exitCode),
  elapsed_seconds: elapsed === "" ? null : Number(elapsed),
  max_rss_bytes: maxRss === "" ? null : Number(maxRss),
  diagnostic_count: Number(diagCount),
  log_path: logPath
};
fs.appendFileSync(samplesPath, JSON.stringify(record) + "\n");
' "$samples_file" "$tool" "$target" "$run" "$tsconfig" "$command" "$completed" "$timed_out" "$exit_code" "$elapsed" "$max_rss" "$diag_count" "$log_path"
}

run_sample() {
    local samples_file="$1"
    local logs_dir="$2"
    local tool="$3"
    local target="$4"
    local run="$5"
    local tsconfig="$6"
    shift 6
    local -a command=( "$@" )

    local log_path="$logs_dir/${target}-${tool}-run${run}.log"
    local escaped_command=""
    printf -v escaped_command '%q ' "${command[@]}"
    escaped_command="${escaped_command% }"

    echo "[${target}] ${tool} run ${run}/${RUNS}"

    set +e
    /usr/bin/time -l zsh -lc \
        "limit addressspace ${ADDRESSSPACE_LIMIT}; limit datasize ${DATASIZE_LIMIT}; export CARGO_BUILD_JOBS=1; export RAYON_NUM_THREADS=2; export NODE_OPTIONS=--max-old-space-size=${NODE_HEAP_MB}; export TSZ_LIB_DIR=$(printf '%q' "$TSZ_LIB_DIR"); perl -e 'alarm shift @ARGV; exec @ARGV' ${TIMEOUT_SECONDS} ${escaped_command}" \
        >"$log_path" 2>&1
    local exit_code=$?
    set -e

    local timed_out=false
    if [ "$exit_code" -eq 142 ]; then
        timed_out=true
    fi

    local completed=true
    if [ "$timed_out" = true ]; then
        completed=false
    fi

    local elapsed=""
    elapsed="$(awk '/ real/ { print $1 }' "$log_path" | tail -n 1)"
    local max_rss=""
    max_rss="$(awk '/maximum resident set size/ { print $1 }' "$log_path" | tail -n 1)"
    local diag_count=0
    diag_count="$(grep -Ec 'error TS[0-9]+:' "$log_path" || true)"

    append_sample \
        "$samples_file" \
        "$tool" \
        "$target" \
        "$run" \
        "$tsconfig" \
        "$escaped_command" \
        "$completed" \
        "$timed_out" \
        "$exit_code" \
        "$elapsed" \
        "$max_rss" \
        "$diag_count" \
        "$log_path"
}

print_summary() {
    node -e '
const fs = require("fs");
const report = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
console.log("");
console.log("Summary:");
for (const row of report.summaries) {
  const elapsed = row.median_elapsed_seconds == null ? "n/a" : `${row.median_elapsed_seconds.toFixed(2)}s`;
  const rss = row.median_max_rss_bytes == null ? "n/a" : `${(row.median_max_rss_bytes / (1024 * 1024)).toFixed(1)} MiB`;
  console.log(
    `${row.target.padEnd(5)} ${row.tool.padEnd(4)} completed=${String(row.completed_runs).padStart(2)}/${row.runs} ` +
    `success=${String(row.successful_runs).padStart(2)}/${row.runs} timeout=${row.timed_out_runs} ` +
    `median=${elapsed} rss=${rss} diagnostics=${row.median_diagnostic_count ?? "n/a"}`
  );
}
' "$1"
}

IFS=',' read -r -a TOOLS <<<"$TOOLS_CSV"
IFS=',' read -r -a TARGETS <<<"$TARGETS_CSV"

timestamp="$(date +%Y%m%d-%H%M%S)"
if [ -z "$OUTPUT_DIR" ]; then
    OUTPUT_DIR="$PROJECT_ROOT/artifacts/perf/large-repo-$timestamp"
fi
mkdir -p "$OUTPUT_DIR/logs"

SAMPLES_FILE="$OUTPUT_DIR/samples.ndjson"
REPORT_FILE="$OUTPUT_DIR/report.json"
touch "$SAMPLES_FILE"

branch="$(git branch --show-current)"
commit="$(git rev-parse HEAD)"
TSZ_VERSION="unavailable"
TSC_VERSION="unavailable"
TSGO_VERSION="unavailable"

if [[ " ${TOOLS[*]} " == *" tsc "* ]]; then
    ensure_tsc
    TSC_VERSION="$(tool_version_line "$TSC")"
fi
if [[ " ${TOOLS[*]} " == *" tsgo "* ]]; then
    ensure_tsgo
    TSGO_VERSION="$(tool_version_line "$TSGO")"
fi
if [[ " ${TOOLS[*]} " == *" tsz "* ]]; then
    ensure_tsz
    TSZ_VERSION="$(tool_version_line "$TSZ_BIN")"
fi

for target in "${TARGETS[@]}"; do
    case "$target" in
        root)
            tsconfig="$ROOT_TSCONFIG"
            ;;
        slice)
            tsconfig="$SLICE_TSCONFIG"
            ;;
        *)
            echo "Unknown target: $target" >&2
            exit 1
            ;;
    esac

    if [ ! -f "$tsconfig" ]; then
        echo "Missing tsconfig for target '$target': $tsconfig" >&2
        exit 1
    fi

    for tool in "${TOOLS[@]}"; do
        for run in $(seq 1 "$RUNS"); do
            case "$tool" in
                tsz)
                    run_sample "$SAMPLES_FILE" "$OUTPUT_DIR/logs" "$tool" "$target" "$run" "$tsconfig" \
                        "$TSZ_BIN" -b -p "$tsconfig" --force --pretty false
                    ;;
                tsc)
                    run_sample "$SAMPLES_FILE" "$OUTPUT_DIR/logs" "$tool" "$target" "$run" "$tsconfig" \
                        "$TSC" -b "$tsconfig" --force --pretty false
                    ;;
                tsgo)
                    run_sample "$SAMPLES_FILE" "$OUTPUT_DIR/logs" "$tool" "$target" "$run" "$tsconfig" \
                        "$TSGO" -b "$tsconfig" --force --pretty false
                    ;;
                *)
                    echo "Unknown tool: $tool" >&2
                    exit 1
                    ;;
            esac
        done
    done
done

TOOL_VERSIONS_JSON="$(node -e 'console.log(JSON.stringify({ tsz: process.argv[1], tsc: process.argv[2], tsgo: process.argv[3] }))' "$TSZ_VERSION" "$TSC_VERSION" "$TSGO_VERSION")"

median_node_script \
    "$SAMPLES_FILE" \
    "$REPORT_FILE" \
    "$branch" \
    "$commit" \
    "$ROOT_TSCONFIG" \
    "$SLICE_TSCONFIG" \
    "$RUNS" \
    "$TIMEOUT_SECONDS" \
    "$TOOLS_CSV" \
    "$TARGETS_CSV" \
    "$ADDRESSSPACE_LIMIT" \
    "$DATASIZE_LIMIT" \
    "$NODE_HEAP_MB" \
    "$TOOL_VERSIONS_JSON"
print_summary "$REPORT_FILE"

echo ""
echo "Report written to $REPORT_FILE"
