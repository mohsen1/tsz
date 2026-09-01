#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

usage() {
  cat <<'EOF'
Usage:
  scripts/safe-run.sh scripts/perf/forced-parallel-project-determinism.sh [options]

Options:
  --row NAME        Project row to check (default: ts-toolbelt-project).
                    Supported: ts-toolbelt-project, utility-types-project,
                    dom-smoke, webworker-smoke, dom-webworker-smoke.
  --tsconfig PATH   Existing tsconfig to check instead of materializing a row.
                    Uses custom-tsconfig as the output label unless --row is
                    also provided as a descriptive label.
  --runs N         Number of baseline and candidate runs per worker width (default: 5).
  --workers LIST   Comma-separated Rayon worker widths (default: 4,8,16).
  --mode MODE      Candidate mode (default: default). Only `default` is
                    available in the replacement compiler; it varies
                    RAYON_NUM_THREADS without changing semantic dispatch.
  --baseline-workers N
                    Set RAYON_NUM_THREADS for baseline runs. For default-path
                    schedule checks, use 1 to compare parallel runs against a
                    single-worker baseline.
  --out DIR        Output directory (default: .target/forced-parallel-determinism).

Environment:
  TSZ_BIN          tsz binary to execute (default: .target/dist-fast/tsz).
  TSZ_PROJECT_COMPILE_FIXTURE_ROOT
                   Fixture root (default: .target/project-compile-guard).
  TSZ_DETERMINISM_TIMEOUT
                   Per-run timeout in seconds (default: 150).

The harness keeps normal dispatch for baseline runs unless --baseline-workers
is set. Candidate runs preserve normal dispatch while varying
RAYON_NUM_THREADS. Each run captures raw stdout+stderr and exit code. A row
passes when every captured byte stream and exit code matches the first
baseline run.
EOF
}

ROW="ts-toolbelt-project"
ROW_WAS_SET=0
TSCONFIG_INPUT=""
RUNS=5
WORKERS="4,8,16"
MODE="default"
BASELINE_WORKERS=""
OUT_ROOT="$ROOT_DIR/.target/forced-parallel-determinism"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --row)
      ROW="${2:-}"
      ROW_WAS_SET=1
      shift 2
      ;;
    --tsconfig)
      TSCONFIG_INPUT="${2:-}"
      shift 2
      ;;
    --runs)
      RUNS="${2:-}"
      shift 2
      ;;
    --workers)
      WORKERS="${2:-}"
      shift 2
      ;;
    --mode)
      MODE="${2:-}"
      shift 2
      ;;
    --baseline-workers)
      BASELINE_WORKERS="${2:-}"
      shift 2
      ;;
    --out)
      OUT_ROOT="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if ! [[ "$RUNS" =~ ^[1-9][0-9]*$ ]]; then
  echo "error: --runs must be a positive integer: $RUNS" >&2
  exit 2
fi

case "$MODE" in
  default)
    ;;
  forced|both)
    echo "error: --mode $MODE belonged to the retired compiler and is unavailable" >&2
    exit 2
    ;;
  *)
    echo "error: --mode must be default: $MODE" >&2
    exit 2
    ;;
esac

if [[ -n "$BASELINE_WORKERS" ]] && ! [[ "$BASELINE_WORKERS" =~ ^[1-9][0-9]*$ ]]; then
  echo "error: --baseline-workers must be a positive integer: $BASELINE_WORKERS" >&2
  exit 2
fi

TSZ_BIN="${TSZ_BIN:-$ROOT_DIR/.target/dist-fast/tsz}"
FIXTURE_ROOT="${TSZ_PROJECT_COMPILE_FIXTURE_ROOT:-$ROOT_DIR/.target/project-compile-guard}"
RUN_TIMEOUT="${TSZ_DETERMINISM_TIMEOUT:-150}"

if [[ ! -x "$TSZ_BIN" ]]; then
  echo "error: TSZ_BIN is not executable: $TSZ_BIN" >&2
  exit 1
fi

# shellcheck source=scripts/bench/project-fixtures.sh
source "$ROOT_DIR/scripts/bench/project-fixtures.sh"
tsz_sync_project_row_groups

mkdir -p "$FIXTURE_ROOT" "$OUT_ROOT"

write_global_lib_smoke_fixture() {
  local name="$1"
  local libs_json="$2"
  local global_interface="$3"
  local global_object="$4"
  local global_property="$5"
  local root="$FIXTURE_ROOT/$name"

  rm -rf "$root"
  mkdir -p "$root"

  cat >"$root/tsconfig.json" <<EOF
{
  "compilerOptions": {
    "target": "es2020",
    "module": "esnext",
    "strict": true,
    "skipLibCheck": true,
    "lib": $libs_json
  },
  "files": ["augment.ts", "use.ts"]
}
EOF

  cat >"$root/augment.ts" <<EOF
declare global {
  interface $global_interface {
    $global_property: string;
  }

  interface Console {
    ${global_property}Log(value: string): void;
  }
}

export {};
EOF

  cat >"$root/use.ts" <<EOF
$global_object.$global_property = "stable";
console.${global_property}Log($global_object.$global_property);
EOF

  printf '%s\n' "$root/tsconfig.json"
}

prepare_row() {
  if [[ -n "$TSCONFIG_INPUT" ]]; then
    if [[ ! -f "$TSCONFIG_INPUT" ]]; then
      echo "error: --tsconfig does not name a file: $TSCONFIG_INPUT" >&2
      exit 2
    fi
    tsz_physical_path_for_maybe_missing "$TSCONFIG_INPUT"
    return 0
  fi

  case "$ROW" in
    ts-toolbelt-project)
      tsz_ensure_git_fixture "ts-toolbelt" "$TS_TOOLBELT_REPO" "$TS_TOOLBELT_REF" "$FIXTURE_ROOT/ts-toolbelt"
      tsz_write_ts_toolbelt_config "$FIXTURE_ROOT/ts-toolbelt/tsconfig.tsz-guard.json"
      printf '%s\n' "$FIXTURE_ROOT/ts-toolbelt/tsconfig.tsz-guard.json"
      ;;
    utility-types-project)
      tsz_ensure_git_fixture "utility-types" "$UTILITY_TYPES_REPO" "$UTILITY_TYPES_REF" "$FIXTURE_ROOT/utility-types"
      tsz_write_utility_types_config "$FIXTURE_ROOT/utility-types/tsconfig.tsz-guard.json"
      printf '%s\n' "$FIXTURE_ROOT/utility-types/tsconfig.tsz-guard.json"
      ;;
    dom-smoke)
      write_global_lib_smoke_fixture \
        "dom-smoke" \
        '["es2020", "dom"]' \
        "Window" \
        "window" \
        "tszDomGateSmoke"
      ;;
    webworker-smoke)
      write_global_lib_smoke_fixture \
        "webworker-smoke" \
        '["es2020", "webworker"]' \
        "WorkerGlobalScope" \
        "self" \
        "tszWebworkerGateSmoke"
      ;;
    dom-webworker-smoke)
      write_global_lib_smoke_fixture \
        "dom-webworker-smoke" \
        '["es2020", "dom", "webworker"]' \
        "Window" \
        "window" \
        "tszDomWebworkerGateSmoke"
      ;;
    *)
      echo "error: unsupported row: $ROW" >&2
      exit 2
      ;;
  esac
}

run_capture() {
  local label="$1"
  local output="$2"
  shift 2

  local rc=0
  if command -v timeout >/dev/null 2>&1; then
    timeout "$RUN_TIMEOUT" "$@" >"$output" 2>&1 || rc=$?
  else
    "$@" >"$output" 2>&1 || rc=$?
  fi
  printf '%s\n' "$rc" >"${output}.rc"
  printf '%s rc=%s\n' "$label" "$rc"
}

compare_capture() {
  local baseline="$1"
  local candidate="$2"
  local label="$3"

  if ! cmp -s "${baseline}.rc" "${candidate}.rc"; then
    echo "mismatch: $label exit code differs from sequential baseline" >&2
    return 1
  fi
  if ! cmp -s "$baseline" "$candidate"; then
    echo "mismatch: $label output differs from sequential baseline" >&2
    return 1
  fi
}

if [[ -n "$TSCONFIG_INPUT" && "$ROW_WAS_SET" == "0" ]]; then
  ROW="custom-tsconfig"
fi

TSCONFIG="$(prepare_row)"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
OUT_DIR="$OUT_ROOT/$ROW/$STAMP"
mkdir -p "$OUT_DIR"

echo "row=$ROW"
echo "tsconfig=$TSCONFIG"
echo "out=$OUT_DIR"
echo "runs=$RUNS"
echo "workers=$WORKERS"
echo "mode=$MODE"
echo "baseline_workers=${BASELINE_WORKERS:-default}"

BASELINE="$OUT_DIR/baseline-1.out"
for run in $(seq 1 "$RUNS"); do
  out="$OUT_DIR/baseline-$run.out"
  if [[ -n "$BASELINE_WORKERS" ]]; then
    run_capture \
      "baseline workers=$BASELINE_WORKERS run $run" \
      "$out" \
      env RAYON_NUM_THREADS="$BASELINE_WORKERS" \
      "$TSZ_BIN" --noEmit -p "$TSCONFIG"
  else
    run_capture "baseline run $run" "$out" "$TSZ_BIN" --noEmit -p "$TSCONFIG"
  fi
  compare_capture "$BASELINE" "$out" "baseline run $run"
done

IFS=',' read -ra WORKER_LIST <<< "$WORKERS"
for worker_count in "${WORKER_LIST[@]}"; do
  if ! [[ "$worker_count" =~ ^[1-9][0-9]*$ ]]; then
    echo "error: worker count must be a positive integer: $worker_count" >&2
    exit 2
  fi
  for run in $(seq 1 "$RUNS"); do
    out="$OUT_DIR/default-workers-${worker_count}-run-${run}.out"
    run_capture \
      "default workers=$worker_count run $run" \
      "$out" \
      env RAYON_NUM_THREADS="$worker_count" \
      "$TSZ_BIN" --noEmit -p "$TSCONFIG"
    compare_capture "$BASELINE" "$out" "default workers=$worker_count run $run"
  done
done

echo "project determinism passed: $OUT_DIR"
