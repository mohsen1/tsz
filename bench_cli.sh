#!/usr/bin/env bash

# Benchmark tsz vs tsc on a repo with timing + memory stats.
# Usage:
#   ./wasm/bench_cli.sh --repo /path/to/repo --tsconfig tsconfig.json
#   ./wasm/bench_cli.sh --repo /path/to/repo --runs 5 --warmup 2
#   TSZ_BIN=/path/to/tsz TSC_BIN=/path/to/tsc ./wasm/bench_cli.sh --repo /path/to/repo

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

usage() {
  cat <<'EOF'
Usage: ./wasm/bench_cli.sh [options]

Options:
  --repo <path>         Repo root to benchmark (default: current directory)
  --tsconfig <path>     tsconfig path relative to repo (default: tsconfig.json)
  --runs <n>            Number of measured runs (default: 3)
  --warmup <n>          Number of warmup runs (default: 1)
  --tsz <path>          Path to tsz binary (default: wasm/target/release/tsz)
  --tsc <path>          Path to tsc executable (default: repo/node_modules/.bin/tsc)
  --no-emit             Add --noEmit to both compilers (default)
  --emit                Do not pass --noEmit
  --tsz-args "<args>"   Extra args for tsz (quoted)
  --tsc-args "<args>"   Extra args for tsc (quoted)
  -h, --help            Show this help

Environment:
  TSZ_BIN, TSC_BIN, TSZ_ARGS, TSC_ARGS
EOF
}

REPO_DIR=""
TSCONFIG="tsconfig.json"
RUNS=3
WARMUP=1
NO_EMIT=1
TSZ_BIN="${TSZ_BIN:-$SCRIPT_DIR/target/release/tsz}"
TSC_BIN="${TSC_BIN:-}"
TSZ_ARGS="${TSZ_ARGS:-}"
TSC_ARGS="${TSC_ARGS:-}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo)
      REPO_DIR="$2"
      shift 2
      ;;
    --tsconfig)
      TSCONFIG="$2"
      shift 2
      ;;
    --runs)
      RUNS="$2"
      shift 2
      ;;
    --warmup)
      WARMUP="$2"
      shift 2
      ;;
    --tsz)
      TSZ_BIN="$2"
      shift 2
      ;;
    --tsc)
      TSC_BIN="$2"
      shift 2
      ;;
    --no-emit)
      NO_EMIT=1
      shift
      ;;
    --emit)
      NO_EMIT=0
      shift
      ;;
    --tsz-args)
      TSZ_ARGS="$2"
      shift 2
      ;;
    --tsc-args)
      TSC_ARGS="$2"
      shift 2
      ;;
    -h|--help)
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

if [[ -z "$REPO_DIR" ]]; then
  REPO_DIR="$(pwd)"
fi

if [[ ! -d "$REPO_DIR" ]]; then
  echo "Repo path not found: $REPO_DIR" >&2
  exit 1
fi

if [[ "$TSCONFIG" = /* ]]; then
  TSCONFIG_PATH="$TSCONFIG"
else
  TSCONFIG_PATH="$REPO_DIR/$TSCONFIG"
fi

if [[ ! -f "$TSCONFIG_PATH" ]]; then
  echo "tsconfig not found: $TSCONFIG_PATH" >&2
  exit 1
fi

if [[ -z "$TSC_BIN" ]]; then
  if [[ -x "$REPO_DIR/node_modules/.bin/tsc" ]]; then
    TSC_BIN="$REPO_DIR/node_modules/.bin/tsc"
  else
    echo "tsc not found. Set TSC_BIN or install dependencies in $REPO_DIR." >&2
    exit 1
  fi
fi

if [[ ! -x "$TSZ_BIN" ]]; then
  echo "tsz binary not found or not executable: $TSZ_BIN" >&2
  echo "Build it with: (cd $PROJECT_ROOT/wasm && cargo build --release --bin tsz)" >&2
  exit 1
fi

if [[ ! -x "$TSC_BIN" ]]; then
  echo "tsc binary not found or not executable: $TSC_BIN" >&2
  exit 1
fi

TIME_STYLE="none"
if [[ -x "/usr/bin/time" ]]; then
  probe="$(mktemp)"
  if /usr/bin/time -v true 2> "$probe"; then
    if grep -q "Maximum resident set size" "$probe"; then
      TIME_STYLE="gnu"
    fi
  fi
  if [[ "$TIME_STYLE" = "none" ]]; then
    if /usr/bin/time -l true 2> "$probe"; then
      if grep -q "maximum resident set size" "$probe"; then
        TIME_STYLE="bsd"
      fi
    fi
  fi
  rm -f "$probe"
fi

parse_elapsed_seconds() {
  local elapsed="$1"
  awk -v t="$elapsed" 'BEGIN{
    n=split(t, parts, ":");
    if (n==3) { print parts[1]*3600+parts[2]*60+parts[3]; }
    else if (n==2) { print parts[1]*60+parts[2]; }
    else { print parts[1]; }
  }'
}

run_once() {
  local label="$1"
  shift
  local time_file log_file
  time_file="$(mktemp)"
  log_file="$(mktemp)"

  if [[ "$TIME_STYLE" = "gnu" ]]; then
    /usr/bin/time -v -o "$time_file" "$@" >"$log_file" 2>&1
  elif [[ "$TIME_STYLE" = "bsd" ]]; then
    /usr/bin/time -l -o "$time_file" "$@" >"$log_file" 2>&1
  else
    local start end
    start="$(date +%s)"
    "$@" >"$log_file" 2>&1
    end="$(date +%s)"
    echo "wall_seconds=$((end - start))" > "$time_file"
  fi

  local status=$?
  if [[ $status -ne 0 ]]; then
    echo "Command failed for $label: $*" >&2
    sed -n '1,200p' "$log_file" >&2
    rm -f "$time_file" "$log_file"
    exit $status
  fi

  local elapsed rss_kb
  if [[ "$TIME_STYLE" = "gnu" ]]; then
    local raw
    raw="$(awk -F': ' '/Elapsed/ {print $2}' "$time_file")"
    elapsed="$(parse_elapsed_seconds "$raw")"
    rss_kb="$(awk -F': ' '/Maximum resident set size/ {print $2}' "$time_file")"
  elif [[ "$TIME_STYLE" = "bsd" ]]; then
    elapsed="$(awk '$1=="real" {print $2; exit} $2=="real" {print $1; exit}' "$time_file")"
    rss_kb="$(awk '/maximum resident set size/ {print int($1/1024)}' "$time_file")"
  else
    elapsed="$(awk -F'=' '/wall_seconds/ {print $2}' "$time_file")"
    rss_kb="0"
  fi

  rm -f "$time_file" "$log_file"
  echo "$elapsed $rss_kb"
}

run_benchmark() {
  local label="$1"
  shift
  local -a cmd=("$@")

  echo "== $label =="
  local i
  for ((i=1; i<=WARMUP; i++)); do
    run_once "$label (warmup $i/$WARMUP)" "${cmd[@]}" >/dev/null
  done

  local total=0
  local best=0
  local max_rss=0
  for ((i=1; i<=RUNS; i++)); do
    read -r elapsed rss_kb < <(run_once "$label (run $i/$RUNS)" "${cmd[@]}")
    total="$(awk -v a="$total" -v b="$elapsed" 'BEGIN{print a+b}')"
    if [[ $i -eq 1 || $(awk -v a="$elapsed" -v b="$best" 'BEGIN{print (a<b)}') -eq 1 ]]; then
      best="$elapsed"
    fi
    if [[ -n "$rss_kb" && "$rss_kb" != "0" ]]; then
      if [[ $rss_kb -gt $max_rss ]]; then
        max_rss="$rss_kb"
      fi
    fi
  done

  local avg
  avg="$(awk -v total="$total" -v runs="$RUNS" 'BEGIN{print total/runs}')"
  printf "avg: %.3fs\n" "$avg"
  printf "best: %.3fs\n" "$best"
  if [[ "$max_rss" != "0" ]]; then
    local mb
    mb="$(awk -v kb="$max_rss" 'BEGIN{print kb/1024}')"
    printf "max_rss: %.1f MiB\n" "$mb"
  else
    echo "max_rss: n/a"
  fi
  echo ""
}

tsz_extra=()
if [[ -n "$TSZ_ARGS" ]]; then
  read -r -a tsz_extra <<< "$TSZ_ARGS"
fi

tsc_extra=()
if [[ -n "$TSC_ARGS" ]]; then
  read -r -a tsc_extra <<< "$TSC_ARGS"
fi

tsz_cmd=("$TSZ_BIN" "--project" "$TSCONFIG_PATH")
if [[ $NO_EMIT -eq 1 ]]; then
  tsz_cmd+=("--noEmit")
fi
if [[ ${#tsz_extra[@]} -gt 0 ]]; then
  tsz_cmd+=("${tsz_extra[@]}")
fi

tsc_cmd=("$TSC_BIN" "--project" "$TSCONFIG_PATH" "--pretty" "false")
if [[ $NO_EMIT -eq 1 ]]; then
  tsc_cmd+=("--noEmit")
fi
if [[ ${#tsc_extra[@]} -gt 0 ]]; then
  tsc_cmd+=("${tsc_extra[@]}")
fi

pushd "$REPO_DIR" >/dev/null
run_benchmark "tsz" "${tsz_cmd[@]}"
run_benchmark "tsc" "${tsc_cmd[@]}"
popd >/dev/null
