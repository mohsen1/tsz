#!/usr/bin/env bash
set -euo pipefail

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

(
  sleep "$timeout_secs"
  kill -KILL "$pid" 2>/dev/null || true
) &
watchdog_pid=$!

exit_code=0
wait "$pid" 2>/dev/null || exit_code=$?

kill "$watchdog_pid" 2>/dev/null || true
wait "$watchdog_pid" 2>/dev/null || true

if [ "$exit_code" -eq 137 ]; then
  exit 124
fi
exit "$exit_code"
