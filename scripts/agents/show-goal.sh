#!/usr/bin/env bash
#
# Print the repo-owned goal file for one multi-agent Codex session.
#
# The preferred source is origin/main so an agent can be redirected by updating
# the repository without merging main into an in-progress feature branch. If the
# file is not present on origin/main yet, fall back to the local checkout.

set -euo pipefail

usage() {
  local stream="${1:-1}"
  cat >&"$stream" <<'USAGE'
usage: scripts/agents/show-goal.sh <AgentName> [--no-fetch|--local]

Examples:
  scripts/agents/show-goal.sh M1-A
  scripts/agents/show-goal.sh Studio-F --no-fetch
  scripts/agents/show-goal.sh Studio-F --local
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ $# -lt 1 || $# -gt 2 ]]; then
  usage 2
  exit 1
fi

AGENT="$1"
if [[ "$AGENT" == --* ]]; then
  echo "unknown argument: $AGENT" >&2
  usage 2
  exit 1
fi

NO_FETCH=false
LOCAL_ONLY=false
if [[ $# -eq 2 ]]; then
  case "$2" in
    --no-fetch) NO_FETCH=true ;;
    --local) LOCAL_ONLY=true ;;
    *) echo "unknown argument: $2" >&2; usage 2; exit 1 ;;
  esac
fi

case "$AGENT" in
  M1-A|M1-B|M1-C|M1-D|M4-A|M4-B|M4-C|M4-D|Studio-A|Studio-B|Studio-C|Studio-D|Studio-E|Studio-F|Reviewer) ;;
  *) echo "unknown AgentName: $AGENT" >&2; exit 1 ;;
esac

ROOT="$(git rev-parse --show-toplevel)"
GOAL_PATH="docs/plan/agents/${AGENT}.md"
REMOTE_GOAL="$(mktemp "${TMPDIR:-/tmp}/tsz-agent-goal.XXXXXX")"
trap 'rm -f "$REMOTE_GOAL"' EXIT

if [[ "$LOCAL_ONLY" == false && "$NO_FETCH" == false ]]; then
  git -C "$ROOT" fetch -q origin main || true
fi

if [[ "$LOCAL_ONLY" == false ]] \
  && git -C "$ROOT" show "origin/main:${GOAL_PATH}" >"$REMOTE_GOAL" 2>/dev/null; then
  cat "$REMOTE_GOAL"
  exit 0
fi

if [[ -f "$ROOT/$GOAL_PATH" ]]; then
  cat "$ROOT/$GOAL_PATH"
  exit 0
fi

echo "goal file not found on origin/main or local checkout: $GOAL_PATH" >&2
exit 1
