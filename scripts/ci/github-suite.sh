#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"
source scripts/ci/suite-metadata.sh
source scripts/ci/ci-resources.sh

suite="${1:?usage: $0 $(ci_suite_usage github)}"
if ! ci_suite_is_known github "$suite"; then
  echo "error: unknown GitHub CI suite '${suite}'" >&2
  echo "valid suites: $(ci_suite_list github ', ')" >&2
  exit 2
fi
export _TSZ_CI_SUITE="$suite"
export TSZ_CI_SUITE="$suite"
ci_report_memory "suite-start-${suite}"
export TSZ_CI_METRICS_DIR="${TSZ_CI_METRICS_DIR:-ci-metrics}"
export TSZ_CI_LOG_DIR="${TSZ_CI_LOG_DIR:-.ci-logs}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-1}"
export CARGO_PROFILE_DIST_FAST_LTO="${CARGO_PROFILE_DIST_FAST_LTO:-false}"
export TSZ_CI_SKIP_HOST_APT="${TSZ_CI_SKIP_HOST_APT:-1}"

mkdir -p "$TSZ_CI_METRICS_DIR" "$TSZ_CI_LOG_DIR" .ci-status

suite_heartbeat_pid=""
start_suite_heartbeat() {
  # TSZ_CI_GITHUB_SUITE_HEARTBEAT_SECONDS controls this outer heartbeat.
  # TSZ_CI_HEARTBEAT_SECONDS controls the per-command heartbeat in run_with_heartbeat.
  local interval="${TSZ_CI_GITHUB_SUITE_HEARTBEAT_SECONDS:-60}"
  (
    while true; do
      sleep "$interval"
      echo "github-suite ${suite} still running at $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    done
  ) &
  suite_heartbeat_pid="$!"
}

stop_suite_heartbeat() {
  if [[ -n "$suite_heartbeat_pid" ]]; then
    kill "$suite_heartbeat_pid" >/dev/null 2>&1 || true
    wait "$suite_heartbeat_pid" 2>/dev/null || true
    suite_heartbeat_pid=""
  fi
}

trap stop_suite_heartbeat EXIT
start_suite_heartbeat

set +e
scripts/ci/full-ci.sh "$suite" 2>&1 | tee "$TSZ_CI_LOG_DIR/full-ci.log"
rc="${PIPESTATUS[0]}"
set -e
printf '%s\n' "$rc" > .ci-status/full-ci.exit

python3 scripts/ci/full-ci-summary.py \
  --suite "$suite" \
  --exit-code "$rc" \
  --metrics-dir "$TSZ_CI_METRICS_DIR" \
  --logs-dir "$TSZ_CI_LOG_DIR" \
  --out .ci-status/check-summary.md || true

if [[ -f .ci-status/check-summary.md ]]; then
  cat .ci-status/check-summary.md >> "${GITHUB_STEP_SUMMARY:-/dev/null}" || true
fi

exit "$rc"
