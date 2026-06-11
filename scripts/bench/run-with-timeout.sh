#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/bench/lib/measure-protocol.sh
source "$SCRIPT_DIR/lib/measure-protocol.sh"

if [ "$#" -lt 2 ]; then
  echo "usage: $0 <seconds> [--] <command...>" >&2
  exit 2
fi

timeout_secs="$1"
shift
if [ "${1:-}" = "--" ]; then
  shift
fi
if [ "$#" -eq 0 ]; then
  echo "usage: $0 <seconds> [--] <command...>" >&2
  exit 2
fi

if ! [[ "$timeout_secs" =~ ^[0-9]+$ ]] || [ "$timeout_secs" -le 0 ]; then
  echo "timeout must be a positive integer number of seconds: $timeout_secs" >&2
  exit 2
fi

"$@" &
pid=$!

# The watchdog samples the process tree's CPU time immediately before the
# kill. On timeout this is the evidence that distinguishes a CPU-bound
# (genuinely slow) run from one starved by CPU contention, whose wall time is
# meaningless (issue #13174). The sample file doubles as the timed-out marker.
cpu_file="$(mktemp)"
rm -f "$cpu_file"
watchdog_pid="$(tsz_start_timeout_watchdog "$timeout_secs" "$pid" "$cpu_file")"

exit_code=0
wait "$pid" 2>/dev/null || exit_code=$?

kill "$watchdog_pid" 2>/dev/null || true
wait "$watchdog_pid" 2>/dev/null || true

if [ "$exit_code" -eq 137 ]; then
  if [ -e "$cpu_file" ]; then
    cpu_secs="$(cat "$cpu_file" 2>/dev/null || true)"
    echo "run-with-timeout: $(tsz_timeout_contention_note "$timeout_secs" "$cpu_secs" \
      "${RUN_WITH_TIMEOUT_MIN_CPU_SHARE_PCT:-$TSZ_MEASURE_DEFAULT_MIN_CPU_SHARE_PCT}")" >&2
  fi
  rm -f "$cpu_file"
  exit 124
fi
rm -f "$cpu_file"
exit "$exit_code"
