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
usage: scripts/agents/show-goal.sh [--json-report PATH] <AgentName> [--no-fetch|--local]

Examples:
  scripts/agents/show-goal.sh M1-A
  scripts/agents/show-goal.sh Studio-manager --no-fetch
  scripts/agents/show-goal.sh Studio-manager --local
  scripts/agents/show-goal.sh Studio-manager --json-report /tmp/tsz-agent-goal.json
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

NO_FETCH=false
LOCAL_ONLY=false
JSON_REPORT=""
AGENT=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-fetch)
      NO_FETCH=true
      shift
      ;;
    --local)
      LOCAL_ONLY=true
      shift
      ;;
    --json-report)
      shift
      if [[ $# -eq 0 ]]; then
        echo "--json-report requires a path (try --help)" >&2
        exit 2
      fi
      JSON_REPORT="$1"
      shift
      ;;
    --json-report=*)
      JSON_REPORT="${1#--json-report=}"
      if [[ -z "$JSON_REPORT" ]]; then
        echo "--json-report requires a path (try --help)" >&2
        exit 2
      fi
      shift
      ;;
    --*)
      echo "unknown argument: $1" >&2
      usage 2
      exit 1
      ;;
    *)
      if [[ -n "$AGENT" ]]; then
        echo "unknown argument: $1" >&2
        usage 2
        exit 1
      fi
      AGENT="$1"
      shift
      ;;
  esac
done

if [[ -z "$AGENT" ]]; then
  usage 2
  exit 1
fi

case "$AGENT" in
  M1-A|M1-B|M1-D|M1-Opus|M4-A|M4-B|M4-C|M4-Opus|Studio-A|Studio-B|Studio-C|Studio-Opus|Studio-manager) ;;
  *) echo "unknown AgentName: $AGENT" >&2; exit 1 ;;
esac

ROOT="$(git rev-parse --show-toplevel)"
GOAL_PATH="docs/plan/agents/${AGENT}.md"
REMOTE_GOAL="$(mktemp "${TMPDIR:-/tmp}/tsz-agent-goal.XXXXXX")"
trap 'rm -f "$REMOTE_GOAL"' EXIT
FETCH_ATTEMPTED=false
PRINTED_SOURCE=""
BRANCH_LOCAL_DIFFERS=false

collect_git_context() {
  GIT_HEAD="$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || echo unknown)"
  GIT_BRANCH="$(git -C "$ROOT" symbolic-ref --short -q HEAD 2>/dev/null || true)"
  GIT_DETACHED=false
  if [[ -z "$GIT_BRANCH" ]]; then
    GIT_DETACHED=true
    if [[ "$GIT_HEAD" == "unknown" ]]; then
      GIT_BRANCH="detached:unknown"
    else
      GIT_BRANCH="detached:${GIT_HEAD:0:12}"
    fi
  fi
  GIT_UPSTREAM="$(git -C "$ROOT" rev-parse --abbrev-ref --symbolic-full-name '@{upstream}' 2>/dev/null || true)"
}

write_json_report() {
  [[ -n "$JSON_REPORT" ]] || return 0
  collect_git_context
  AGENT="$AGENT" \
  ROOT="$ROOT" \
  GOAL_PATH="$GOAL_PATH" \
  PRINTED_SOURCE="$PRINTED_SOURCE" \
  FETCH_ATTEMPTED="$FETCH_ATTEMPTED" \
  LOCAL_ONLY="$LOCAL_ONLY" \
  NO_FETCH="$NO_FETCH" \
  BRANCH_LOCAL_DIFFERS="$BRANCH_LOCAL_DIFFERS" \
  GIT_HEAD="$GIT_HEAD" \
  GIT_BRANCH="$GIT_BRANCH" \
  GIT_DETACHED="$GIT_DETACHED" \
  GIT_UPSTREAM="$GIT_UPSTREAM" \
  JSON_REPORT="$JSON_REPORT" \
  node <<'NODE'
const fs = require("fs");
const path = require("path");

const bool = (value) => value === "true";
const report = {
  ok: true,
  status: "pass",
  agent_goal_status: "pass",
  generated_by: "scripts/agents/show-goal.sh",
  agent: process.env.AGENT,
  repo: process.env.ROOT,
  goal_path: process.env.GOAL_PATH,
  printed_source: process.env.PRINTED_SOURCE,
  fetch_attempted: bool(process.env.FETCH_ATTEMPTED),
  local_only: bool(process.env.LOCAL_ONLY),
  no_fetch: bool(process.env.NO_FETCH),
  branch_local_differs: bool(process.env.BRANCH_LOCAL_DIFFERS),
  git_context: {
    head: process.env.GIT_HEAD,
    branch: process.env.GIT_BRANCH,
    detached: bool(process.env.GIT_DETACHED),
    upstream: process.env.GIT_UPSTREAM || null,
  },
};

fs.mkdirSync(path.dirname(process.env.JSON_REPORT), { recursive: true });
fs.writeFileSync(process.env.JSON_REPORT, `${JSON.stringify(report, null, 2)}\n`);
NODE
}

if [[ "$LOCAL_ONLY" == false && "$NO_FETCH" == false ]]; then
  FETCH_ATTEMPTED=true
  git -C "$ROOT" fetch -q origin main || true
fi

if [[ "$LOCAL_ONLY" == false ]] \
  && git -C "$ROOT" show "origin/main:${GOAL_PATH}" >"$REMOTE_GOAL" 2>/dev/null; then
  if [[ -f "$ROOT/$GOAL_PATH" ]] && ! cmp -s "$REMOTE_GOAL" "$ROOT/$GOAL_PATH"; then
    BRANCH_LOCAL_DIFFERS=true
    echo "warning: printed origin/main:${GOAL_PATH}; branch-local ${GOAL_PATH} differs. Use --local to inspect it." >&2
  fi
  PRINTED_SOURCE="origin/main"
  cat "$REMOTE_GOAL"
  write_json_report
  exit 0
fi

if [[ -f "$ROOT/$GOAL_PATH" ]]; then
  PRINTED_SOURCE="local"
  cat "$ROOT/$GOAL_PATH"
  write_json_report
  exit 0
fi

echo "goal file not found on origin/main or local checkout: $GOAL_PATH" >&2
exit 1
