#!/usr/bin/env bash
set -Eeuo pipefail

: "${GITHUB_REPO:?GITHUB_REPO is required, e.g. mohsen1/tsz}"
: "${GITHUB_TOKEN:?GITHUB_TOKEN is required}"

RUNNER_LABELS="${RUNNER_LABELS:-tsz-cloud-run}"
RUNNER_GROUP="${RUNNER_GROUP:-Default}"
RUNNER_PREFIX="${RUNNER_PREFIX:-cloud-run}"
RUNNER_SUFFIX="${K_REVISION:-rev}-${HOSTNAME:-host}"
RUNNER_NAME="${RUNNER_NAME:-${RUNNER_PREFIX}-${RUNNER_SUFFIX}}"
GITHUB_REPO_URL="https://github.com/${GITHUB_REPO}"
RUNNER_DIR="${RUNNER_DIR:-/home/runner}"

cd "$RUNNER_DIR"

cleanup() {
  echo "Removing GitHub runner ${RUNNER_NAME}"
  ./config.sh remove --unattended --pat "${GITHUB_TOKEN}" || true
}
trap 'cleanup; exit 130' INT
trap 'cleanup; exit 143' TERM
trap 'cleanup; exit 0' EXIT

if [[ -f .runner ]]; then
  ./config.sh remove --unattended --pat "${GITHUB_TOKEN}" || true
fi

echo "Registering GitHub runner ${RUNNER_NAME} for ${GITHUB_REPO_URL} labels=${RUNNER_LABELS}"
./config.sh \
  --unattended \
  --url "${GITHUB_REPO_URL}" \
  --pat "${GITHUB_TOKEN}" \
  --name "${RUNNER_NAME}" \
  --runnergroup "${RUNNER_GROUP}" \
  --labels "${RUNNER_LABELS}" \
  --work /home/runner/_work \
  --ephemeral \
  --replace

./run.sh &
wait $!
