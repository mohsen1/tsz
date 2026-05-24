#!/usr/bin/env bash
set -Eeuo pipefail

: "${GITHUB_REPO:?GITHUB_REPO is required, e.g. mohsen1/tsz}"
: "${GITHUB_TOKEN:?GITHUB_TOKEN is required}"

RUNNER_LABELS="${RUNNER_LABELS:-tsz-cloud-run}"
RUNNER_GROUP="${RUNNER_GROUP:-Default}"
RUNNER_PREFIX="${RUNNER_PREFIX:-cloud-run}"
RUNNER_UUID="$(cat /proc/sys/kernel/random/uuid 2>/dev/null | tr -d '-' | cut -c1-12 || date +%s%N)"
RUNNER_SUFFIX="${RUNNER_UUID}"
RUNNER_NAME="${RUNNER_NAME:-${RUNNER_PREFIX}-${RUNNER_SUFFIX}}"
GITHUB_REPO_URL="https://github.com/${GITHUB_REPO}"
RUNNER_DIR="${RUNNER_DIR:-/home/runner}"
export DISABLE_RUNNER_UPDATE="${DISABLE_RUNNER_UPDATE:-1}"
RUNNER_STARTUP_JITTER_SECONDS="${RUNNER_STARTUP_JITTER_SECONDS:-120}"
RUNNER_CONFIG_RETRIES="${RUNNER_CONFIG_RETRIES:-4}"
RUNNER_CONFIG_RETRY_SLEEP_SECONDS="${RUNNER_CONFIG_RETRY_SLEEP_SECONDS:-30}"
RUNNER_CONFIG_FAILURE_SLEEP_SECONDS="${RUNNER_CONFIG_FAILURE_SLEEP_SECONDS:-300}"

cd "$RUNNER_DIR"

remove_runner() {
  ./config.sh remove --pat "${GITHUB_TOKEN}" || true
}

cleanup() {
  echo "Removing GitHub runner ${RUNNER_NAME}"
  remove_runner
}
trap 'cleanup; exit 130' INT
trap 'cleanup; exit 143' TERM

if [[ -f .runner ]]; then
  remove_runner
fi

if [[ "${RUNNER_STARTUP_JITTER_SECONDS}" =~ ^[0-9]+$ ]] && (( RUNNER_STARTUP_JITTER_SECONDS > 0 )); then
  jitter=$((RANDOM % (RUNNER_STARTUP_JITTER_SECONDS + 1)))
  echo "Waiting ${jitter}s before GitHub runner registration"
  sleep "${jitter}"
fi

echo "Registering GitHub runner ${RUNNER_NAME} for ${GITHUB_REPO_URL} labels=${RUNNER_LABELS}"
attempt=1
while true; do
  if ./config.sh \
    --unattended \
    --url "${GITHUB_REPO_URL}" \
    --pat "${GITHUB_TOKEN}" \
    --name "${RUNNER_NAME}" \
    --runnergroup "${RUNNER_GROUP}" \
    --labels "${RUNNER_LABELS}" \
    --work /home/runner/_work \
    --disableupdate \
    --ephemeral; then
    break
  fi

  if (( attempt >= RUNNER_CONFIG_RETRIES )); then
    echo "GitHub runner registration failed after ${attempt} attempt(s); sleeping ${RUNNER_CONFIG_FAILURE_SLEEP_SECONDS}s before exit"
    sleep "${RUNNER_CONFIG_FAILURE_SLEEP_SECONDS}"
    exit 1
  fi

  sleep_for=$((RUNNER_CONFIG_RETRY_SLEEP_SECONDS * attempt + RANDOM % 10))
  echo "GitHub runner registration failed on attempt ${attempt}; retrying in ${sleep_for}s"
  sleep "${sleep_for}"
  attempt=$((attempt + 1))
done

./run.sh &
wait $!
