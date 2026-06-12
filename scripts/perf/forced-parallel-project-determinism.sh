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
                    Supported: ts-toolbelt-project, utility-types-project.
  --runs N         Number of sequential and forced runs per worker width (default: 5).
  --workers LIST   Comma-separated Rayon worker widths (default: 4,8,16).
  --out DIR        Output directory (default: .target/forced-parallel-determinism).

Environment:
  TSZ_BIN          tsz binary to execute (default: .target/dist-fast/tsz).
  TSZ_PROJECT_COMPILE_FIXTURE_ROOT
                   Fixture root (default: .target/project-compile-guard).
  TSZ_DETERMINISM_TIMEOUT
                   Per-run timeout in seconds (default: 150).

The harness keeps the normal sequential DOM/webworker gate for baseline runs,
then sets TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK=1 for forced-parallel runs. Each
run captures raw stdout+stderr and exit code. A row passes when every captured
byte stream and exit code matches the first sequential baseline run.
EOF
}

ROW="ts-toolbelt-project"
RUNS=5
WORKERS="4,8,16"
OUT_ROOT="$ROOT_DIR/.target/forced-parallel-determinism"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --row)
      ROW="${2:-}"
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

prepare_row() {
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

TSCONFIG="$(prepare_row)"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
OUT_DIR="$OUT_ROOT/$ROW/$STAMP"
mkdir -p "$OUT_DIR"

echo "row=$ROW"
echo "tsconfig=$TSCONFIG"
echo "out=$OUT_DIR"
echo "runs=$RUNS"
echo "workers=$WORKERS"

BASELINE="$OUT_DIR/sequential-1.out"
for run in $(seq 1 "$RUNS"); do
  out="$OUT_DIR/sequential-$run.out"
  run_capture "sequential run $run" "$out" "$TSZ_BIN" --noEmit -p "$TSCONFIG"
  compare_capture "$BASELINE" "$out" "sequential run $run"
done

IFS=',' read -ra WORKER_LIST <<< "$WORKERS"
for worker_count in "${WORKER_LIST[@]}"; do
  if ! [[ "$worker_count" =~ ^[1-9][0-9]*$ ]]; then
    echo "error: worker count must be a positive integer: $worker_count" >&2
    exit 2
  fi
  for run in $(seq 1 "$RUNS"); do
    out="$OUT_DIR/forced-workers-${worker_count}-run-${run}.out"
    run_capture \
      "forced workers=$worker_count run $run" \
      "$out" \
      env TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK=1 RAYON_NUM_THREADS="$worker_count" \
      "$TSZ_BIN" --noEmit -p "$TSCONFIG"
    compare_capture "$BASELINE" "$out" "forced workers=$worker_count run $run"
  done
done

echo "forced-parallel determinism passed: $OUT_DIR"
