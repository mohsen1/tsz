#!/usr/bin/env bash
# Sound perf measurement for a tsz binary on shared boxes (issue #13174).
#
# Implements the measurement protocol from scripts/bench/lib/measure-protocol.sh:
#   1. Never measure a live shared binary path: the binary is snapshotted to an
#      immutable content-addressed copy (hash-verified) and the copy is run, so
#      a sibling session rebuilding dist-fast/ mid-run cannot poison the
#      measurement.
#   2. Never trust wall time alone: each run records process CPU time next to
#      wall time. A wall timeout whose CPU share is below the contention
#      threshold is classified unmeasured-contention, not slow.
#
# Usage:
#   scripts/bench/measure-tsz.sh [options] -- <tsz args...>
#
# Options:
#   --bin PATH            binary to measure (default: $CARGO_TARGET_DIR or
#                         .target, + /dist-fast/tsz)
#   --timeout SECS        wall timeout per run (default: 420)
#   --runs N              repetitions (default: 1)
#   --min-cpu-share PCT   contention threshold percent (default: 25)
#   --snapshot-dir DIR    snapshot location (default: $TMPDIR/tsz-measure-snapshots)
#   --json-file FILE      also write a JSON result artifact
#   --label NAME          free-form label recorded in the artifact
#
# Exit codes:
#   0    every run was measured (the child's own exit code is data, not failure)
#   124  at least one CPU-bound (genuine) timeout and no unmeasured run
#   125  at least one run was unmeasured (contention / missing CPU evidence)
#   2    usage or setup error
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
# shellcheck source=scripts/bench/lib/measure-protocol.sh
source "$SCRIPT_DIR/lib/measure-protocol.sh"

usage() {
  # Print the header comment block (everything between the shebang and the
  # first non-comment line) so the docs above stay the single source of truth.
  awk 'NR > 1 { if (!/^#/) exit; sub(/^# ?/, ""); print }' "${BASH_SOURCE[0]}"
}

DEFAULT_TARGET_DIR="${CARGO_TARGET_DIR:-$PROJECT_ROOT/.target}"
BIN="$DEFAULT_TARGET_DIR/dist-fast/tsz"
TIMEOUT_SECS=420
RUNS=1
MIN_CPU_SHARE_PCT="$TSZ_MEASURE_DEFAULT_MIN_CPU_SHARE_PCT"
SNAPSHOT_DIR="${TSZ_MEASURE_SNAPSHOT_DIR:-${TMPDIR:-/tmp}/tsz-measure-snapshots}"
JSON_FILE=""
LABEL=""

while [ "$#" -gt 0 ]; do
  case "$1" in
    --bin) BIN="$2"; shift 2 ;;
    --timeout) TIMEOUT_SECS="$2"; shift 2 ;;
    --runs) RUNS="$2"; shift 2 ;;
    --min-cpu-share) MIN_CPU_SHARE_PCT="$2"; shift 2 ;;
    --snapshot-dir) SNAPSHOT_DIR="$2"; shift 2 ;;
    --json-file) JSON_FILE="$2"; shift 2 ;;
    --label) LABEL="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    --) shift; break ;;
    *) echo "measure-tsz: unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [ "$#" -eq 0 ]; then
  echo "measure-tsz: no command arguments after --" >&2
  usage >&2
  exit 2
fi
if ! [[ "$TIMEOUT_SECS" =~ ^[0-9]+$ ]] || [ "$TIMEOUT_SECS" -le 0 ]; then
  echo "measure-tsz: --timeout must be a positive integer: $TIMEOUT_SECS" >&2
  exit 2
fi
if ! [[ "$RUNS" =~ ^[0-9]+$ ]] || [ "$RUNS" -le 0 ]; then
  echo "measure-tsz: --runs must be a positive integer: $RUNS" >&2
  exit 2
fi

now_seconds() {
  if [ -n "${EPOCHREALTIME:-}" ]; then
    printf '%s' "${EPOCHREALTIME/,/.}"
  else
    date +%s
  fi
}

# Parse the children line of bash's `times` builtin ("0m0.012s 0m0.004s")
# into "user sys" seconds.
times_children_seconds() {
  { times; } | sed -n '2p' | awk '{
    for (i = 1; i <= 2; i += 1) {
      gsub(/s$/, "", $i)
      split($i, p, "m")
      out[i] = p[1] * 60 + p[2]
    }
    printf "%.3f %.3f", out[1], out[2]
  }'
}

json_escape() {
  local s="$1"
  s="${s//\\/\\\\}"
  s="${s//\"/\\\"}"
  s="${s//$'\n'/\\n}"
  s="${s//$'\t'/\\t}"
  printf '%s' "$s"
}

GIT_SHA="$(git -C "$PROJECT_ROOT" rev-parse --short=12 HEAD 2>/dev/null || echo unknown)"
if [ "$GIT_SHA" != "unknown" ] && [ -n "$(git -C "$PROJECT_ROOT" status --porcelain 2>/dev/null | head -1)" ]; then
  GIT_SHA="${GIT_SHA}-dirty"
fi

snapshot_out="$(tsz_snapshot_binary "$BIN" "$SNAPSHOT_DIR")" || exit 2
read -r SNAPSHOT_BIN BIN_SHA256 <<< "$snapshot_out"
tsz_prune_binary_snapshots "$BIN" "$SNAPSHOT_DIR" "$SNAPSHOT_BIN"

OUT_DIR="$(mktemp -d "${TMPDIR:-/tmp}/tsz-measure.XXXXXX")"

echo "measure-tsz: git $GIT_SHA"
echo "measure-tsz: source binary  $BIN"
echo "measure-tsz: snapshot       $SNAPSHOT_BIN"
echo "measure-tsz: sha256         $BIN_SHA256"
echo "measure-tsz: timeout ${TIMEOUT_SECS}s, runs $RUNS, contention threshold ${MIN_CPU_SHARE_PCT}%"

RUN_ROWS=()
N_MEASURED=0
N_TIMEOUT_CPU_BOUND=0
N_UNMEASURED=0

for run in $(seq 1 "$RUNS"); do
  log="$OUT_DIR/run-${run}.log"
  cpu_file="$OUT_DIR/run-${run}.cpu"
  rm -f "$cpu_file"

  read -r u_before s_before <<< "$(times_children_seconds)"
  t0="$(now_seconds)"

  "$SNAPSHOT_BIN" "$@" > "$log" 2>&1 &
  pid=$!
  watchdog_pid="$(tsz_start_timeout_watchdog "$TIMEOUT_SECS" "$pid" "$cpu_file")"

  rc=0
  wait "$pid" 2>/dev/null || rc=$?
  kill "$watchdog_pid" 2>/dev/null || true
  wait "$watchdog_pid" 2>/dev/null || true

  t1="$(now_seconds)"
  read -r u_after s_after <<< "$(times_children_seconds)"
  wall="$(awk -v a="$t0" -v b="$t1" 'BEGIN { printf "%.2f", b - a }')"

  cpu_user="" cpu_sys="" cpu_total="" classification="" note=""
  if [ "$rc" -eq 137 ] && [ -e "$cpu_file" ]; then
    rc=124
    cpu_total="$(cat "$cpu_file" 2>/dev/null || true)"
    note="$(tsz_timeout_contention_note "$TIMEOUT_SECS" "$cpu_total" "$MIN_CPU_SHARE_PCT")"
    if [ -z "$cpu_total" ]; then
      classification="unmeasured-no-cpu-sample"
      N_UNMEASURED=$((N_UNMEASURED + 1))
    elif tsz_timeout_is_contended "$TIMEOUT_SECS" "$cpu_total" "$MIN_CPU_SHARE_PCT"; then
      classification="unmeasured-contention"
      N_UNMEASURED=$((N_UNMEASURED + 1))
    else
      classification="timeout-cpu-bound"
      N_TIMEOUT_CPU_BOUND=$((N_TIMEOUT_CPU_BOUND + 1))
    fi
  else
    cpu_user="$(awk -v a="$u_before" -v b="$u_after" 'BEGIN { printf "%.3f", b - a }')"
    cpu_sys="$(awk -v a="$s_before" -v b="$s_after" 'BEGIN { printf "%.3f", b - a }')"
    cpu_total="$(awk -v u="$cpu_user" -v s="$cpu_sys" 'BEGIN { printf "%.3f", u + s }')"
    classification="measured"
    N_MEASURED=$((N_MEASURED + 1))
  fi
  share="$(tsz_cpu_share_pct "$cpu_total" "$wall")"
  if [ "$classification" = "measured" ] && [ -n "$share" ] && [ "$share" -lt "$MIN_CPU_SHARE_PCT" ]; then
    note="completed under CPU contention (~${share}% CPU share): wall time unreliable, use cpu_s"
  fi

  echo "measure-tsz: run ${run}/${RUNS}: ${classification} exit=${rc} wall=${wall}s cpu=${cpu_total:-?}s share=${share:-?}% log=$log"
  [ -n "$note" ] && echo "measure-tsz:   ${note}"

  RUN_ROWS+=("$(printf '{"run":%s,"classification":"%s","exit_code":%s,"wall_s":%s,"cpu_s":%s,"cpu_user_s":%s,"cpu_sys_s":%s,"cpu_share_pct":%s,"log":"%s"}' \
    "$run" "$classification" "$rc" "$wall" \
    "${cpu_total:-null}" "${cpu_user:-null}" "${cpu_sys:-null}" "${share:-null}" \
    "$(json_escape "$log")")")
done

if [ -n "$JSON_FILE" ]; then
  cmd_json=""
  for arg in "$@"; do
    cmd_json="${cmd_json:+$cmd_json,}\"$(json_escape "$arg")\""
  done
  runs_json="$(IFS=,; printf '%s' "${RUN_ROWS[*]}")"
  printf '{"protocol":"snapshot+cpu-share/v1","label":"%s","git_sha":"%s","binary":{"source":"%s","snapshot":"%s","sha256":"%s"},"timeout_s":%s,"min_cpu_share_pct":%s,"command":[%s],"runs":[%s],"summary":{"measured":%s,"timeout_cpu_bound":%s,"unmeasured":%s}}\n' \
    "$(json_escape "$LABEL")" "$GIT_SHA" \
    "$(json_escape "$BIN")" "$(json_escape "$SNAPSHOT_BIN")" "$BIN_SHA256" \
    "$TIMEOUT_SECS" "$MIN_CPU_SHARE_PCT" "$cmd_json" "$runs_json" \
    "$N_MEASURED" "$N_TIMEOUT_CPU_BOUND" "$N_UNMEASURED" > "$JSON_FILE"
  echo "measure-tsz: JSON written to $JSON_FILE"
fi

if [ "$N_UNMEASURED" -gt 0 ]; then
  echo "measure-tsz: result UNMEASURED -- do not report these wall times as a regression" >&2
  exit 125
fi
if [ "$N_TIMEOUT_CPU_BOUND" -gt 0 ]; then
  exit 124
fi
exit 0
