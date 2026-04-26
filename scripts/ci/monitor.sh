#!/usr/bin/env bash
# TSZ CI health dashboard.
# Usage: scripts/ci/monitor.sh [--watch] [--json]
set -euo pipefail

REPO="${GITHUB_REPO:-mohsen1/tsz}"
REGION="${REGION:-us-central1}"
POOL_NAME="${POOL_NAME:-tsz-gh-runner}"
CREMA_SERVICE="${CREMA_SERVICE:-tsz-gh-runner-crema}"
WATCH=false
JSON_OUT=false

for arg in "$@"; do
  case "$arg" in
    --watch) WATCH=true ;;
    --json)  JSON_OUT=true ;;
    *)       echo "usage: $0 [--watch] [--json]" >&2; exit 1 ;;
  esac
done

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'

die() { echo "error: $*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "$1 not found in PATH"; }
need gh; need gcloud; need python3

runner_data() {
  gh api "repos/${REPO}/actions/runners" --paginate --jq '.runners[]' 2>/dev/null
}

runs_by_status() {
  gh api "repos/${REPO}/actions/runs?status=${1}&per_page=50" \
    --jq '.workflow_runs[]' 2>/dev/null
}

worker_pool_instances() {
  gcloud beta run worker-pools describe "$POOL_NAME" \
    --region="$REGION" \
    --format="value(metadata.annotations['run.googleapis.com/manualInstanceCount'])" \
    2>/dev/null || echo "?"
}

crema_ready() {
  gcloud run services describe "$CREMA_SERVICE" \
    --region="$REGION" \
    --format="value(status.conditions[0].status)" \
    2>/dev/null || echo "?"
}

snapshot() {
  local ts
  ts=$(date -u '+%Y-%m-%dT%H:%M:%SZ')

  # Runner stats
  local runners
  runners=$(runner_data)
  local total online busy idle offline
  total=$(echo "$runners" | grep -c '"id"' 2>/dev/null || echo 0)
  online=$(echo "$runners" | python3 -c "
import json,sys
r=[json.loads(l) for l in sys.stdin if l.strip()]
print(sum(1 for x in r if x.get('status')=='online'))
" 2>/dev/null || echo 0)
  busy=$(echo "$runners" | python3 -c "
import json,sys
r=[json.loads(l) for l in sys.stdin if l.strip()]
print(sum(1 for x in r if x.get('status')=='online' and x.get('busy')))
" 2>/dev/null || echo 0)
  idle=$(( online - busy ))
  offline=$(( total - online ))

  # Workflow runs
  local in_prog_raw queued_raw
  in_prog_raw=$(runs_by_status in_progress)
  queued_raw=$(runs_by_status queued)

  local in_prog_count queued_count
  in_prog_count=$(echo "$in_prog_raw" | python3 -c "
import json,sys; lines=[l for l in sys.stdin if l.strip()]; print(len(lines))
" 2>/dev/null || echo 0)
  queued_count=$(echo "$queued_raw" | python3 -c "
import json,sys; lines=[l for l in sys.stdin if l.strip()]; print(len(lines))
" 2>/dev/null || echo 0)

  # Per-run job status (queued jobs waiting for runners)
  local waiting_jobs=0
  if [[ -n "$in_prog_raw" || -n "$queued_raw" ]]; then
    waiting_jobs=$(echo "$in_prog_raw $queued_raw" | python3 - <<'EOF'
import json, sys, subprocess

# Read run IDs from stdin (each line is a JSON object)
lines = sys.stdin.read().split()
# Actually we can't run gh from inside python easily; print 0
print(0)
EOF
    2>/dev/null || echo 0)
  fi

  # GCP state
  local pool_instances crema_ok
  pool_instances=$(worker_pool_instances)
  crema_ok=$(crema_ready)

  if $JSON_OUT; then
    python3 -c "
import json
print(json.dumps({
  'ts': '${ts}',
  'runners': {'total': ${total}, 'online': ${online}, 'busy': ${busy}, 'idle': ${idle}, 'offline': ${offline}},
  'runs': {'in_progress': ${in_prog_count}, 'queued': ${queued_count}},
  'pool_instances': '${pool_instances}',
  'crema_ready': '${crema_ok}',
}))
"
    return
  fi

  # Human-readable output
  echo ""
  echo -e "${BOLD}TSZ CI Health Dashboard${RESET}  ${CYAN}${ts}${RESET}"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

  echo -e "\n${BOLD}Self-Hosted Runners${RESET}"
  local runner_status="${GREEN}healthy${RESET}"
  [[ "$idle" -eq 0 && "$in_prog_count" -gt 0 ]] && runner_status="${YELLOW}saturated${RESET}"
  [[ "$offline" -gt 10 ]] && runner_status="${RED}degraded${RESET}"
  printf "  %-12s %s\n" "Status:" "$(echo -e "$runner_status")"
  printf "  %-12s %s\n" "Online:" "${online} / ${total}"
  printf "  %-12s %s\n" "Busy:" "${busy}"
  printf "  %-12s %s\n" "Idle:" "${idle}"
  [[ "$offline" -gt 0 ]] && printf "  %-12s %s\n" "Offline:" "$(echo -e "${RED}${offline}${RESET}")"

  echo -e "\n${BOLD}Workflow Runs${RESET}"
  printf "  %-16s %s\n" "In progress:" "${in_prog_count}"
  printf "  %-16s %s\n" "Queued:" "${queued_count}"
  [[ "${queued_count}" -gt 0 ]] && echo -e "  ${YELLOW}⚠ Queued runs are waiting for available runners${RESET}"

  if [[ -n "$in_prog_raw" ]]; then
    echo ""
    echo -e "  ${CYAN}Active runs:${RESET}"
    echo "$in_prog_raw" | python3 -c "
import json, sys, datetime
for line in sys.stdin:
    line = line.strip()
    if not line: continue
    r = json.loads(line)
    sha = r.get('head_sha','')[:7]
    name = r.get('name','')[:28]
    created = r.get('created_at','')
    pr = r.get('pull_requests',[])
    ref = f\"PR#{pr[0]['number']}\" if pr else r.get('head_branch','')[:20]
    print(f\"    #{r['id']}  {name:<28}  {sha}  {ref}\")
" 2>/dev/null || true
  fi

  if [[ -n "$queued_raw" ]]; then
    echo ""
    echo -e "  ${YELLOW}Queued runs (waiting for runners):${RESET}"
    echo "$queued_raw" | python3 -c "
import json, sys
for line in sys.stdin:
    line = line.strip()
    if not line: continue
    r = json.loads(line)
    sha = r.get('head_sha','')[:7]
    name = r.get('name','')[:28]
    pr = r.get('pull_requests',[])
    ref = f\"PR#{pr[0]['number']}\" if pr else r.get('head_branch','')[:20]
    print(f\"    #{r['id']}  {name:<28}  {sha}  {ref}\")
" 2>/dev/null || true
  fi

  echo -e "\n${BOLD}GCP Infrastructure${RESET}"
  printf "  %-22s %s\n" "Worker pool instances:" "${pool_instances}"
  local crema_display
  [[ "$crema_ok" == "True" ]] && crema_display="${GREEN}Ready${RESET}" || crema_display="${RED}${crema_ok}${RESET}"
  printf "  %-22s %s\n" "CREMA autoscaler:" "$(echo -e "$crema_display")"

  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
}

if $WATCH; then
  while true; do
    clear
    snapshot
    echo -e "\n  ${CYAN}Refreshing every 30s — Ctrl-C to exit${RESET}"
    sleep 30
  done
else
  snapshot
fi
