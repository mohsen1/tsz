#!/usr/bin/env bash
#
# List open issues and PRs owned by one or all multi-agent labels.

set -euo pipefail

AGENTS=(
  M1-A M1-B M1-D M1-Opus
  M4-A M4-B M4-C M4-Opus
  Studio-A Studio-B Studio-C Studio-Opus
  Studio-manager
)

usage() {
  cat <<'USAGE'
usage: scripts/agents/list-owned-work.sh [--pr-state] [--json-report PATH] [AgentName|--all]

Lists owned open PRs/issues and prints compact per-agent summary counters.

Examples:
  scripts/agents/list-owned-work.sh M1-A
  scripts/agents/list-owned-work.sh --all
  scripts/agents/list-owned-work.sh --pr-state Studio-manager
  scripts/agents/list-owned-work.sh Studio-manager --json-report /tmp/tsz-owned-work.json
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

WITH_PR_STATE=false
JSON_REPORT=""
POSITIONAL=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --pr-state|--with-pr-state)
      WITH_PR_STATE=true
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
REPORT_ROWS=""

collect_git_context() {
  GIT_HEAD="$(git rev-parse HEAD 2>/dev/null || echo unknown)"
  GIT_BRANCH="$(git symbolic-ref --short -q HEAD 2>/dev/null || true)"
  GIT_DETACHED=false
  if [[ -z "$GIT_BRANCH" ]]; then
    GIT_DETACHED=true
    if [[ "$GIT_HEAD" == "unknown" ]]; then
      GIT_BRANCH="detached:unknown"
    else
      GIT_BRANCH="detached:${GIT_HEAD:0:12}"
    fi
  fi
  GIT_UPSTREAM="$(git rev-parse --abbrev-ref --symbolic-full-name '@{upstream}' 2>/dev/null || true)"
}

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

count_rows() {
  local rows="$1"
  if [[ -z "$rows" ]]; then
    echo 0
    return
  fi
  printf '%s\n' "$rows" | awk 'NF { count++ } END { print count + 0 }'
}

count_pr_state_rows() {
  local rows="$1"
  local state="$2"
  if [[ -z "$rows" ]]; then
    echo 0
    return
  fi
  printf '%s\n' "$rows" | awk -v state="$state" '$2 == state { count++ } END { print count + 0 }'
}

count_pr_token_rows() {
  local rows="$1"
  local token="$2"
  if [[ -z "$rows" ]]; then
    echo 0
    return
  fi
  printf '%s\n' "$rows" | awk -v token="$token" '
    {
      for (i = 1; i <= NF; i++) {
        if ($i == token) {
          count++
          next
        }
      }
    }
    END { print count + 0 }
  '
}

count_ready_unqueued_pr_rows() {
  local rows="$1"
  if [[ -z "$rows" ]]; then
    echo 0
    return
  fi
  printf '%s\n' "$rows" | awk '
    $2 == "ready" {
      for (i = 1; i <= NF; i++) {
        if ($i == "mergeQueue=off") {
          count++
          next
        }
      }
    }
    END { print count + 0 }
  '
}

count_merge_queue_tested_pr_rows() {
  local rows="$1"
  if [[ -z "$rows" ]]; then
    echo 0
    return
  fi
  printf '%s\n' "$rows" | awk '
    {
      queued = 0
      queue_success = 0
      for (i = 1; i <= NF; i++) {
        if ($i == "mergeQueue=on") {
          queued = 1
        } else if ($i == "queue=success") {
          queue_success = 1
        }
      }
      if (queued && queue_success) {
        count++
      }
    }
    END { print count + 0 }
  '
}

count_merge_queue_unverified_pr_rows() {
  local rows="$1"
  if [[ -z "$rows" ]]; then
    echo 0
    return
  fi
  printf '%s\n' "$rows" | awk '
    {
      queued = 0
      queue_seen = 0
      queue_success = 0
      for (i = 1; i <= NF; i++) {
        if ($i == "mergeQueue=on") {
          queued = 1
        } else if ($i ~ /^queue=/) {
          queue_seen = 1
          if ($i == "queue=success") {
            queue_success = 1
          }
        }
      }
      if (queued && queue_seen && !queue_success) {
        count++
      }
    }
    END { print count + 0 }
  '
}

json_array_from_lines() {
  local rows="$1"
  ROWS="$rows" node <<'NODE'
const rows = (process.env.ROWS ?? "").split(/\n/).filter(Boolean);
process.stdout.write(JSON.stringify(rows));
NODE
}

for agent in "${SELECTED[@]}"; do
  case "$agent" in
    M1-A|M1-B|M1-D|M1-Opus|M4-A|M4-B|M4-C|M4-Opus|Studio-A|Studio-B|Studio-C|Studio-Opus|Studio-manager) ;;
    *) echo "unknown AgentName: $agent" >&2; exit 1 ;;
  esac

  label="agent:${agent}"
  echo "## $label"
  echo ""
  echo "PRs:"
  if [[ "$WITH_PR_STATE" == true ]]; then
    prs="$(
      gh pr list --state open --limit 100 --label "$label" \
        --json number,title,isDraft,url,mergeStateStatus,mergeable,autoMergeRequest,statusCheckRollup,labels \
        --jq '
          def queue_state:
            ([.statusCheckRollup[]? | select((.__typename == "StatusContext" and .context == "Queue Tested") or .name == "Queue Tested")] | first) as $queue |
            if $queue == null then "queue=none"
            elif $queue.__typename == "StatusContext" then "queue=\(($queue.state // "unknown") | ascii_downcase)"
            else "queue=\((($queue.conclusion // $queue.status // "unknown")) | ascii_downcase)"
            end;
          def merge_queue_label:
            if any(.labels[]?; .name == "merge-queue") then "mergeQueue=on" else "mergeQueue=off" end;
          .[] |
            "#\(.number) " +
            (if .isDraft then "draft" else "ready" end) +
            " merge=\(.mergeStateStatus // "UNKNOWN")" +
            " mergeable=\(.mergeable // "UNKNOWN")" +
            " autoMerge=" + (if .autoMergeRequest then "on" else "off" end) +
            " " + merge_queue_label +
            " " + queue_state +
            " " + .title + " " + .url
        ' \
        2>/dev/null ||
        list_owned_items_rest "$label" pr
    )"
  else
    prs="$(
      gh pr list --state open --limit 100 --label "$label" \
        --json number,title,isDraft,url,labels,autoMergeRequest,statusCheckRollup \
        --jq '
          def queue_state:
            ([.statusCheckRollup[]? | select((.__typename == "StatusContext" and .context == "Queue Tested") or .name == "Queue Tested")] | first) as $queue |
            if $queue == null then "queue=none"
            elif $queue.__typename == "StatusContext" then "queue=\(($queue.state // "unknown") | ascii_downcase)"
            else "queue=\((($queue.conclusion // $queue.status // "unknown")) | ascii_downcase)"
            end;
          def merge_queue_label:
            if any(.labels[]?; .name == "merge-queue") then "mergeQueue=on" else "mergeQueue=off" end;
          .[] |
            "#\(.number) " +
            (if .isDraft then "draft" else "ready" end) +
            " autoMerge=" + (if .autoMergeRequest then "on" else "off" end) +
            " " + merge_queue_label +
            " " + queue_state +
            " " + .title + " " + .url
        ' \
        2>/dev/null ||
        list_owned_items_rest "$label" pr
    )"
  fi
  if [[ -n "$prs" ]]; then
    printf '%s\n' "$prs"
  else
    echo "- none"
  fi
  pr_count="$(count_rows "$prs")"
  ready_pr_count="$(count_pr_state_rows "$prs" ready)"
  draft_pr_count="$(count_pr_state_rows "$prs" draft)"
  auto_merge_pr_count="$(count_pr_token_rows "$prs" "autoMerge=on")"
  merge_queue_pr_count="$(count_pr_token_rows "$prs" "mergeQueue=on")"
  merge_queue_tested_pr_count="$(count_merge_queue_tested_pr_rows "$prs")"
  merge_queue_unverified_pr_count="$(count_merge_queue_unverified_pr_rows "$prs")"
  ready_unqueued_pr_count="$(count_ready_unqueued_pr_rows "$prs")"
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
  issue_count="$(count_rows "$issues")"
  total_count=$(( pr_count + issue_count ))
  if (( total_count == 0 )); then
    owned_work_status="clear"
  else
    owned_work_status="active"
  fi
  echo ""
  echo "owned_pr_count=$pr_count"
  echo "owned_ready_pr_count=$ready_pr_count"
  echo "owned_draft_pr_count=$draft_pr_count"
  echo "owned_auto_merge_pr_count=$auto_merge_pr_count"
  echo "owned_merge_queue_pr_count=$merge_queue_pr_count"
  echo "owned_merge_queue_tested_pr_count=$merge_queue_tested_pr_count"
  echo "owned_merge_queue_unverified_pr_count=$merge_queue_unverified_pr_count"
  echo "owned_ready_unqueued_pr_count=$ready_unqueued_pr_count"
  echo "owned_issue_count=$issue_count"
  echo "owned_work_status=$owned_work_status"
  echo ""

  if [[ -n "$JSON_REPORT" ]]; then
    pr_json="$(json_array_from_lines "$prs")"
    issue_json="$(json_array_from_lines "$issues")"
    report_row="$(
      ROW_AGENT="$agent" \
      ROW_LABEL="$label" \
      ROW_PR_COUNT="$pr_count" \
      ROW_READY_PR_COUNT="$ready_pr_count" \
      ROW_DRAFT_PR_COUNT="$draft_pr_count" \
      ROW_AUTO_MERGE_PR_COUNT="$auto_merge_pr_count" \
      ROW_MERGE_QUEUE_PR_COUNT="$merge_queue_pr_count" \
      ROW_MERGE_QUEUE_TESTED_PR_COUNT="$merge_queue_tested_pr_count" \
      ROW_MERGE_QUEUE_UNVERIFIED_PR_COUNT="$merge_queue_unverified_pr_count" \
      ROW_READY_UNQUEUED_PR_COUNT="$ready_unqueued_pr_count" \
      ROW_ISSUE_COUNT="$issue_count" \
      ROW_STATUS="$owned_work_status" \
      ROW_PRS="$pr_json" \
      ROW_ISSUES="$issue_json" \
      node <<'NODE'
const row = {
  agent: process.env.ROW_AGENT,
  label: process.env.ROW_LABEL,
  prs: JSON.parse(process.env.ROW_PRS ?? "[]"),
  issues: JSON.parse(process.env.ROW_ISSUES ?? "[]"),
  pr_count: Number(process.env.ROW_PR_COUNT ?? 0),
  ready_pr_count: Number(process.env.ROW_READY_PR_COUNT ?? 0),
  draft_pr_count: Number(process.env.ROW_DRAFT_PR_COUNT ?? 0),
  auto_merge_pr_count: Number(process.env.ROW_AUTO_MERGE_PR_COUNT ?? 0),
  merge_queue_pr_count: Number(process.env.ROW_MERGE_QUEUE_PR_COUNT ?? 0),
  merge_queue_tested_pr_count: Number(process.env.ROW_MERGE_QUEUE_TESTED_PR_COUNT ?? 0),
  merge_queue_unverified_pr_count: Number(process.env.ROW_MERGE_QUEUE_UNVERIFIED_PR_COUNT ?? 0),
  ready_unqueued_pr_count: Number(process.env.ROW_READY_UNQUEUED_PR_COUNT ?? 0),
  issue_count: Number(process.env.ROW_ISSUE_COUNT ?? 0),
  owned_work_clear: process.env.ROW_STATUS === "clear",
  owned_work_status: process.env.ROW_STATUS,
};
process.stdout.write(`${JSON.stringify(row)}\n`);
NODE
    )"
    REPORT_ROWS+="$report_row"$'\n'
  fi
done

if [[ -n "$JSON_REPORT" ]]; then
  collect_git_context
  REPOSITORY="$REPOSITORY" \
  WITH_PR_STATE="$WITH_PR_STATE" \
  REPORT_ROWS="$REPORT_ROWS" \
  GIT_HEAD="$GIT_HEAD" \
  GIT_BRANCH="$GIT_BRANCH" \
  GIT_DETACHED="$GIT_DETACHED" \
  GIT_UPSTREAM="$GIT_UPSTREAM" \
  JSON_REPORT="$JSON_REPORT" \
  node <<'NODE'
const fs = require("fs");
const path = require("path");

const agents = (process.env.REPORT_ROWS ?? "")
  .split(/\n/)
  .filter(Boolean)
  .map((line) => JSON.parse(line));
const totalPrCount = agents.reduce((sum, agent) => sum + Number(agent.pr_count ?? 0), 0);
const totalReadyPrCount = agents.reduce(
  (sum, agent) => sum + Number(agent.ready_pr_count ?? 0),
  0,
);
const totalDraftPrCount = agents.reduce(
  (sum, agent) => sum + Number(agent.draft_pr_count ?? 0),
  0,
);
const totalMergeQueuePrCount = agents.reduce(
  (sum, agent) => sum + Number(agent.merge_queue_pr_count ?? 0),
  0,
);
const totalAutoMergePrCount = agents.reduce(
  (sum, agent) => sum + Number(agent.auto_merge_pr_count ?? 0),
  0,
);
const totalMergeQueueTestedPrCount = agents.reduce(
  (sum, agent) => sum + Number(agent.merge_queue_tested_pr_count ?? 0),
  0,
);
const totalMergeQueueUnverifiedPrCount = agents.reduce(
  (sum, agent) => sum + Number(agent.merge_queue_unverified_pr_count ?? 0),
  0,
);
const totalReadyUnqueuedPrCount = agents.reduce(
  (sum, agent) => sum + Number(agent.ready_unqueued_pr_count ?? 0),
  0,
);
const totalIssueCount = agents.reduce(
  (sum, agent) => sum + Number(agent.issue_count ?? 0),
  0,
);
const totalOwnedCount = totalPrCount + totalIssueCount;
const ownedWorkClear = totalOwnedCount === 0;

const report = {
  ok: ownedWorkClear,
  status: ownedWorkClear ? "clear" : "active",
  generated_by: "scripts/agents/list-owned-work.sh",
  repository: process.env.REPOSITORY,
  with_pr_state: process.env.WITH_PR_STATE === "true",
  git_context: {
    head: process.env.GIT_HEAD,
    branch: process.env.GIT_BRANCH,
    detached: process.env.GIT_DETACHED === "true",
    upstream: process.env.GIT_UPSTREAM || null,
  },
  owned_work_clear: ownedWorkClear,
  owned_work_status: ownedWorkClear ? "clear" : "active",
  total_pr_count: totalPrCount,
  total_ready_pr_count: totalReadyPrCount,
  total_draft_pr_count: totalDraftPrCount,
  total_auto_merge_pr_count: totalAutoMergePrCount,
  total_merge_queue_pr_count: totalMergeQueuePrCount,
  total_merge_queue_tested_pr_count: totalMergeQueueTestedPrCount,
  total_merge_queue_unverified_pr_count: totalMergeQueueUnverifiedPrCount,
  total_ready_unqueued_pr_count: totalReadyUnqueuedPrCount,
  total_issue_count: totalIssueCount,
  total_owned_count: totalOwnedCount,
  agents,
};

fs.mkdirSync(path.dirname(process.env.JSON_REPORT), { recursive: true });
fs.writeFileSync(process.env.JSON_REPORT, `${JSON.stringify(report, null, 2)}\n`);
NODE
fi
