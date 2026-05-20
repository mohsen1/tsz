#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"
source scripts/ci/suite-metadata.sh

suite="${1:?usage: $0 $(ci_suite_usage github)}"
if ! ci_suite_is_known github "$suite"; then
  echo "error: unknown GitHub CI suite '${suite}'" >&2
  echo "valid suites: $(ci_suite_list github ', ')" >&2
  exit 2
fi
export _TSZ_CI_SUITE="$suite"
export TSZ_CI_SUITE="$suite"
export _TSZ_CI_CACHE_BUCKET="${_TSZ_CI_CACHE_BUCKET:-${TSZ_CI_CACHE_BUCKET:-gs://thirdface-ai-oauth_cloudbuild/tsz-ci-cache}}"
export TSZ_CI_CACHE_BUCKET="$_TSZ_CI_CACHE_BUCKET"
export TSZ_CI_METRICS_DIR="${TSZ_CI_METRICS_DIR:-.ci-metrics}"
export TSZ_CI_LOG_DIR="${TSZ_CI_LOG_DIR:-.ci-logs}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-1}"
export CARGO_PROFILE_DIST_FAST_LTO="${CARGO_PROFILE_DIST_FAST_LTO:-false}"
export TSZ_CI_SKIP_HOST_APT="${TSZ_CI_SKIP_HOST_APT:-1}"

mkdir -p "$TSZ_CI_METRICS_DIR" "$TSZ_CI_LOG_DIR" .ci-status

suite_heartbeat_pid=""
start_suite_heartbeat() {
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

restore_rc=0
if [[ "${TSZ_CI_CACHE_RESTORE:-1}" == "1" ]]; then
  if command -v gsutil >/dev/null 2>&1; then
    scripts/ci/gcp-cache.sh restore || restore_rc="$?"
  else
    echo "warning: gsutil is unavailable; skipping GCS CI cache restore" >&2
  fi
  if [[ "$restore_rc" -ne 0 ]]; then
    echo "warning: CI cache restore failed with rc=${restore_rc}; continuing" >&2
  fi
else
  echo "info: GCS cache restore skipped (TSZ_CI_CACHE_RESTORE=0)"
fi

set +e
scripts/ci/gcp-full-ci.sh "$suite" 2>&1 | tee "$TSZ_CI_LOG_DIR/full-ci.log"
rc="${PIPESTATUS[0]}"
set -e
printf '%s\n' "$rc" > .ci-status/full-ci.exit

python3 scripts/ci/gcp-summary.py \
  --suite "$suite" \
  --exit-code "$rc" \
  --metrics-dir "$TSZ_CI_METRICS_DIR" \
  --logs-dir "$TSZ_CI_LOG_DIR" \
  --out .ci-status/check-summary.md || true

if [[ "${TSZ_CI_CACHE_SAVE:-1}" != "1" ]]; then
  echo "info: GCS cache save skipped (TSZ_CI_CACHE_SAVE=0)"
elif [[ "$rc" -ne 0 ]]; then
  # A failed suite often leaves a partially-populated target dir
  # (some workspace crates compiled, some not, fingerprints written
  # mid-flight, etc.). Publishing that as the new shared cache for the
  # next build to restore is exactly the kind of "stale forever" state
  # the new write policy was built to prevent. Skip cache save on
  # non-zero suite exit so main's blob always reflects a green build.
  # TSZ_CI_CACHE_SAVE_ON_FAILURE=1 escapes the gate for emergency
  # repairs (e.g., a known-good build that fails on a flaky test).
  if [[ "${TSZ_CI_CACHE_SAVE_ON_FAILURE:-0}" == "1" ]]; then
    echo "info: suite failed (rc=${rc}) but TSZ_CI_CACHE_SAVE_ON_FAILURE=1 — saving cache anyway"
    if command -v gsutil >/dev/null 2>&1; then
      scripts/ci/gcp-cache.sh save || echo "warning: CI cache save failed" >&2
    else
      echo "warning: gsutil is unavailable; skipping GCS CI cache save" >&2
    fi
  else
    echo "info: GCS cache save skipped (suite failed with rc=${rc})"
  fi
elif command -v gsutil >/dev/null 2>&1; then
  scripts/ci/gcp-cache.sh save || echo "warning: CI cache save failed" >&2
else
  echo "warning: gsutil is unavailable; skipping GCS CI cache save" >&2
fi

if [[ -f .ci-status/check-summary.md ]]; then
  cat .ci-status/check-summary.md >> "${GITHUB_STEP_SUMMARY:-/dev/null}" || true
fi

exit "$rc"
