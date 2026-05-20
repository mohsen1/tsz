#!/usr/bin/env bash
#
# List open issues and PRs owned by one or all multi-agent labels.

set -euo pipefail

AGENTS=(
  M1-A M1-B M1-C M1-D
  M4-A M4-B M4-C M4-D
  Studio-A Studio-B Studio-C Studio-D Studio-E Studio-F
  Reviewer
)

usage() {
  cat <<'USAGE'
usage: scripts/agents/list-owned-work.sh [AgentName|--all]

Examples:
  scripts/agents/list-owned-work.sh M1-A
  scripts/agents/list-owned-work.sh --all
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ $# -eq 0 || "${1:-}" == "--all" ]]; then
  SELECTED=("${AGENTS[@]}")
else
  SELECTED=("$1")
fi

for agent in "${SELECTED[@]}"; do
  case "$agent" in
    M1-A|M1-B|M1-C|M1-D|M4-A|M4-B|M4-C|M4-D|Studio-A|Studio-B|Studio-C|Studio-D|Studio-E|Studio-F|Reviewer) ;;
    *) echo "unknown AgentName: $agent" >&2; exit 1 ;;
  esac

  label="agent:${agent}"
  echo "## $label"
  echo ""
  echo "PRs:"
  prs="$(gh pr list --state open --limit 100 --label "$label" --json number,title,isDraft,url \
    --jq '.[] | "#\(.number) " + (if .isDraft then "draft" else "ready" end) + " " + .title + " " + .url')"
  if [[ -n "$prs" ]]; then
    printf '%s\n' "$prs"
  else
    echo "- none"
  fi
  echo ""
  echo "Issues:"
  issues="$(gh issue list --state open --limit 100 --label "$label" --json number,title,url \
    --jq '.[] | "#\(.number) " + .title + " " + .url')"
  if [[ -n "$issues" ]]; then
    printf '%s\n' "$issues"
  else
    echo "- none"
  fi
  echo ""
done
