#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

suite="${1:?usage: $0 <build|dist-binaries|unit-archive|node-harness-prep|lint|unit|unit-shard|wasm|wasm-web|conformance|conformance-aggregate|emit|emit-shard|emit-aggregate|fourslash|fourslash-shard|fourslash-aggregate>}"
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

if [[ "${TSZ_CI_CACHE_SAVE:-1}" == "1" ]]; then
  if command -v gsutil >/dev/null 2>&1; then
    scripts/ci/gcp-cache.sh save || echo "warning: CI cache save failed" >&2
  else
    echo "warning: gsutil is unavailable; skipping GCS CI cache save" >&2
  fi
else
  echo "info: GCS cache save skipped (TSZ_CI_CACHE_SAVE=0)"
fi

if [[ -f .ci-status/check-summary.md ]]; then
  cat .ci-status/check-summary.md >> "${GITHUB_STEP_SUMMARY:-/dev/null}" || true
fi

exit "$rc"
