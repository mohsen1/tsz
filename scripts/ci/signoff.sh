#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

usage() {
  cat <<'EOF'
usage: scripts/ci/signoff.sh [--no-post]

Runs the local PR signoff suite from a clean checkout and posts a GitHub commit
status named "signoff" for HEAD. Branch protection can require the PR Signoff
job, which checks this status instead of spending hosted runner minutes.

Environment:
  SIGNOFF_COMMANDS_FILE  newline-delimited commands to run instead of defaults
  SIGNOFF_CONTEXT        commit status context (default: signoff)
EOF
}

post_status=true
while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --no-post)
      post_status=false
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument '$1'" >&2
      usage >&2
      exit 2
      ;;
  esac
done

context="${SIGNOFF_CONTEXT:-signoff}"
sha="$(git rev-parse HEAD)"
user="$(git config user.name || true)"
if [[ -z "$user" && "$post_status" == "true" ]]; then
  user="$(gh api user --jq .login 2>/dev/null || echo unknown)"
fi
[[ -n "$user" ]] || user="local"

red=$'\033[0;31m'
green=$'\033[0;32m'
blue=$'\033[0;34m'
reset=$'\033[0m'
announce() {
  local color="$1"
  shift
  printf '%s%s%s\n' "$color" "$*" "$reset"
}

post_commit_status() {
  local state="$1"
  local description="$2"
  [[ "$post_status" == "true" ]] || return 0
  local owner
  local repo
  owner="$(gh repo view --json owner --jq .owner.login)"
  repo="$(gh repo view --json name --jq .name)"
  gh api \
    --method POST --silent \
    -H "Accept: application/vnd.github+json" \
    -H "X-GitHub-Api-Version: 2022-11-28" \
    "/repos/${owner}/${repo}/statuses/${sha}" \
    -f "context=${context}" \
    -f "state=${state}" \
    -f "description=${description}"
}

if [[ -n "$(git status --porcelain)" ]]; then
  announce "$red" "Can't sign off on a dirty repository."
  git status --short
  exit 1
fi

commands=()
if [[ -n "${SIGNOFF_COMMANDS_FILE:-}" ]]; then
  while IFS= read -r line; do
    [[ -n "$line" ]] || continue
    [[ "$line" =~ ^[[:space:]]*# ]] && continue
    commands+=("$line")
  done < "$SIGNOFF_COMMANDS_FILE"
else
  commands+=("cargo fmt --all --check")
  # Active rewrite tests are one strict workspace pass. The retained legacy
  # corpus is disabled until cases are ported through the public API; there is
  # no known-failures allowance for tests that have joined this suite.
  commands+=("scripts/test/nextest-guard.sh -- scripts/safe-run.sh -- scripts/ci/unit-nextest.sh --junit-dir .ci-logs/signoff-unit-junit --workspace")
fi

announce "$green" "Attempting to sign off on ${sha} as ${user}."
SECONDS=0

for command in "${commands[@]}"; do
  split="$SECONDS"
  announce "$blue" "Run ${command}"
  if ! bash -lc "$command"; then
    elapsed="$SECONDS"
    post_commit_status "failure" "Signoff failed after ${elapsed}s"
    announce "$red" "Signoff failed while running: ${command}"
    exit 1
  fi
  announce "$green" "Completed ${command} in $((SECONDS - split)) seconds"
done

post_commit_status "success" "Signed off by ${user} (${SECONDS}s)"
announce "$green" "Signed off on ${sha} in ${SECONDS} seconds."
