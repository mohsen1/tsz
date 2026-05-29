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
usage: scripts/agents/list-owned-work.sh [--pr-state] [AgentName|--all]

Examples:
  scripts/agents/list-owned-work.sh M1-A
  scripts/agents/list-owned-work.sh --all
  scripts/agents/list-owned-work.sh --pr-state Studio-F
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

WITH_PR_STATE=false
POSITIONAL=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --pr-state|--with-pr-state)
      WITH_PR_STATE=true
      shift
      ;;
    --all)
      POSITIONAL+=("$1")
      shift
      ;;
    -*)
      echo "Unknown option: $1 (try --help)" >&2
      exit 2
      ;;
    *)
      POSITIONAL+=("$1")
      shift
      ;;
  esac
done

if [[ ${#POSITIONAL[@]} -gt 1 ]]; then
  echo "Unknown option: ${POSITIONAL[1]} (try --help)" >&2
  exit 2
fi

if [[ ${#POSITIONAL[@]} -eq 0 || "${POSITIONAL[0]:-}" == "--all" ]]; then
  SELECTED=("${AGENTS[@]}")
else
  SELECTED=("${POSITIONAL[0]}")
fi

REPOSITORY="${GITHUB_REPOSITORY:-mohsen1/tsz}"

list_owned_items_rest() {
  local label="$1"
  local kind="$2"
  local rows

  rows="$(
    gh api "repos/${REPOSITORY}/issues?state=open&labels=${label}&per_page=100" \
      --jq '.[] | select(if "'"$kind"'" == "pr" then has("pull_request") else has("pull_request") | not end) | [.number, .title, .html_url] | @tsv'
  )" || return 1

  if [[ "$kind" == "issue" ]]; then
    if [[ -n "$rows" ]]; then
      while IFS=$'\t' read -r number title url; do
        [[ -n "$number" ]] || continue
        printf '#%s %s %s\n' "$number" "$title" "$url"
      done <<< "$rows"
    fi
    return 0
  fi

  if [[ -n "$rows" ]]; then
    while IFS=$'\t' read -r number title url; do
      [[ -n "$number" ]] || continue
      local draft="false"
      draft="$(gh api "repos/${REPOSITORY}/pulls/${number}" --jq '.draft' 2>/dev/null || echo false)"
      if [[ "$draft" == "true" ]]; then
        printf '#%s draft %s %s\n' "$number" "$title" "$url"
      else
        printf '#%s ready %s %s\n' "$number" "$title" "$url"
      fi
    done <<< "$rows"
  fi
}

for agent in "${SELECTED[@]}"; do
  case "$agent" in
    M1-A|M1-B|M1-C|M1-D|M4-A|M4-B|M4-C|M4-D|Studio-A|Studio-B|Studio-C|Studio-D|Studio-E|Studio-F|Reviewer) ;;
    *) echo "unknown AgentName: $agent" >&2; exit 1 ;;
  esac

  label="agent:${agent}"
  echo "## $label"
  echo ""
  echo "PRs:"
  if [[ "$WITH_PR_STATE" == true ]]; then
    prs="$(
      gh pr list --state open --limit 100 --label "$label" \
        --json number,title,isDraft,url,mergeStateStatus,mergeable,autoMergeRequest,statusCheckRollup \
        --jq '
          def queue_state:
            ([.statusCheckRollup[]? | select((.__typename == "StatusContext" and .context == "Queue Tested") or .name == "Queue Tested")] | first) as $queue |
            if $queue == null then "queue=none"
            elif $queue.__typename == "StatusContext" then "queue=\(($queue.state // "unknown") | ascii_downcase)"
            else "queue=\((($queue.conclusion // $queue.status // "unknown")) | ascii_downcase)"
            end;
          .[] |
            "#\(.number) " +
            (if .isDraft then "draft" else "ready" end) +
            " merge=\(.mergeStateStatus // "UNKNOWN")" +
            " mergeable=\(.mergeable // "UNKNOWN")" +
            " autoMerge=" + (if .autoMergeRequest then "on" else "off" end) +
            " " + queue_state +
            " " + .title + " " + .url
        ' \
        2>/dev/null ||
        list_owned_items_rest "$label" pr
    )"
  else
    prs="$(
      gh pr list --state open --limit 100 --label "$label" --json number,title,isDraft,url \
        --jq '.[] | "#\(.number) " + (if .isDraft then "draft" else "ready" end) + " " + .title + " " + .url' \
        2>/dev/null ||
        list_owned_items_rest "$label" pr
    )"
  fi
  if [[ -n "$prs" ]]; then
    printf '%s\n' "$prs"
  else
    echo "- none"
  fi
  echo ""
  echo "Issues:"
  issues="$(
    gh issue list --state open --limit 100 --label "$label" --json number,title,url \
      --jq '.[] | "#\(.number) " + .title + " " + .url' \
      2>/dev/null ||
      list_owned_items_rest "$label" issue
  )"
  if [[ -n "$issues" ]]; then
    printf '%s\n' "$issues"
  else
    echo "- none"
  fi
  echo ""
done
