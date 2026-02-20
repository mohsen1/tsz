#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/gemini-fleet-tmux.sh [start|stop|status|attach] [options]

Actions:
  start            Start fleet (default)
  stop             Stop fleet tmux session
  status           Show fleet status
  attach           Attach to tmux session

Options:
  --session-name NAME        tmux session name (default: gemini-fleet)
  --mode MODE                conformance|emit|lsp|arch (default: conformance)
  --worker-model MODEL       Worker model for scripts/gemini-loop.sh (default: gemini-3.1-pro)
  --manager-model MODEL      Manager model for AI decisions (default: same as worker)
  --model-candidates CSV     Fallback order if model unavailable
  --disable-manager          Disable AI manager decisions
  --manager-interval SEC     AI decision cadence (default: 120)
  --manager-timeout SEC      Per decision timeout (default: 30)
  --prompt-file FILE         Override prompt file for workers
  --monitor-interval SEC     Supervisor heartbeat interval (default: 30)
  --idle-restart-sec SEC     Hard restart if no worker log updates (default: 1200)
  --cleanup-interval-sec SEC Periodic artifact cleanup cadence (default: 1800)
  --cleanup-stale-days DAYS  cleanup-build-artifacts --stale value (default: 1)
  --loop-timeout SEC         GEMINI_LOOP_TIMEOUT for workers (default: 420)
  --loop-sleep SEC           GEMINI_LOOP_SLEEP for workers (default: 5)
  --repeat-window LINES      Tail window for repeat detection (default: 120)
  --repeat-threshold COUNT   Hard restart threshold for repeated lines (default: 90)
  --no-cleanup               Disable periodic artifact cleanup
  --no-runaway-guard         Disable periodic runaway process checks
  --repo-glob GLOB           Sibling repo name glob under parent dir (default: tsz*)
  -h, --help                 Show help

Examples:
  scripts/gemini-fleet-tmux.sh start --mode conformance
  scripts/gemini-fleet-tmux.sh status
  scripts/gemini-fleet-tmux.sh stop
USAGE
}

ACTION="start"
SESSION_NAME="${GEMINI_FLEET_SESSION_NAME:-gemini-fleet}"
MODE="${GEMINI_FLEET_MODE:-conformance}"
WORKER_MODEL="${GEMINI_FLEET_WORKER_MODEL:-gemini-3.1-pro-preview}"
MANAGER_MODEL="${GEMINI_FLEET_MANAGER_MODEL:-}"
MODEL_CANDIDATES="${GEMINI_FLEET_MODEL_CANDIDATES:-gemini-3.1-pro-preview,gemini-2.5-pro,gemini-2.5-flash,gemini-2.0-flash}"
MANAGER_ENABLED=true
MANAGER_INTERVAL="${GEMINI_FLEET_MANAGER_INTERVAL:-120}"
MANAGER_TIMEOUT="${GEMINI_FLEET_MANAGER_TIMEOUT:-30}"
MODEL_PROBE_TIMEOUT="${GEMINI_FLEET_MODEL_PROBE_TIMEOUT:-20}"
PROMPT_FILE=""
MONITOR_INTERVAL="${GEMINI_FLEET_MONITOR_INTERVAL:-30}"
IDLE_RESTART_SEC="${GEMINI_FLEET_IDLE_RESTART_SEC:-1200}"
CLEANUP_INTERVAL_SEC="${GEMINI_FLEET_CLEANUP_INTERVAL_SEC:-1800}"
CLEANUP_STALE_DAYS="${GEMINI_FLEET_CLEANUP_STALE_DAYS:-1}"
LOOP_TIMEOUT_SEC="${GEMINI_FLEET_LOOP_TIMEOUT_SEC:-420}"
LOOP_SLEEP_SEC="${GEMINI_FLEET_LOOP_SLEEP_SEC:-5}"
REPEAT_WINDOW_LINES="${GEMINI_FLEET_REPEAT_WINDOW_LINES:-120}"
REPEAT_THRESHOLD="${GEMINI_FLEET_REPEAT_THRESHOLD:-90}"
RUN_CLEANUP=true
RUN_RUNAWAY_GUARD=true
REPO_GLOB="${GEMINI_FLEET_REPO_GLOB:-tsz*}"

if [[ $# -gt 0 ]]; then
  case "$1" in
    start|stop|status|attach)
      ACTION="$1"
      shift
      ;;
  esac
fi

while [[ $# -gt 0 ]]; do
  case "$1" in
    --session-name)
      SESSION_NAME="${2:-}"
      shift 2
      ;;
    --mode)
      MODE="${2:-}"
      shift 2
      ;;
    --worker-model)
      WORKER_MODEL="${2:-}"
      shift 2
      ;;
    --manager-model)
      MANAGER_MODEL="${2:-}"
      shift 2
      ;;
    --model-candidates)
      MODEL_CANDIDATES="${2:-}"
      shift 2
      ;;
    --disable-manager)
      MANAGER_ENABLED=false
      shift
      ;;
    --manager-interval)
      MANAGER_INTERVAL="${2:-}"
      shift 2
      ;;
    --manager-timeout)
      MANAGER_TIMEOUT="${2:-}"
      shift 2
      ;;
    --prompt-file)
      PROMPT_FILE="${2:-}"
      shift 2
      ;;
    --monitor-interval)
      MONITOR_INTERVAL="${2:-}"
      shift 2
      ;;
    --idle-restart-sec)
      IDLE_RESTART_SEC="${2:-}"
      shift 2
      ;;
    --cleanup-interval-sec)
      CLEANUP_INTERVAL_SEC="${2:-}"
      shift 2
      ;;
    --cleanup-stale-days)
      CLEANUP_STALE_DAYS="${2:-}"
      shift 2
      ;;
    --loop-timeout)
      LOOP_TIMEOUT_SEC="${2:-}"
      shift 2
      ;;
    --loop-sleep)
      LOOP_SLEEP_SEC="${2:-}"
      shift 2
      ;;
    --repeat-window)
      REPEAT_WINDOW_LINES="${2:-}"
      shift 2
      ;;
    --repeat-threshold)
      REPEAT_THRESHOLD="${2:-}"
      shift 2
      ;;
    --no-cleanup)
      RUN_CLEANUP=false
      shift
      ;;
    --no-runaway-guard)
      RUN_RUNAWAY_GUARD=false
      shift
      ;;
    --repo-glob)
      REPO_GLOB="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PARENT_DIR="$(dirname "$REPO_ROOT")"
FLEET_LOG_DIR="$REPO_ROOT/logs/gemini-fleet"
GEMINI_CONTROL_DIR="${GEMINI_FLEET_CONTROL_DIR:-$HOME/.gemini/tmp/fleet-manager}"
mkdir -p "$FLEET_LOG_DIR"
mkdir -p "$GEMINI_CONTROL_DIR"
SUPERVISOR_LOG="$FLEET_LOG_DIR/supervisor.$(date +%Y%m%d-%H%M%S).log"

log() {
  local line="[$(date '+%Y-%m-%d %H:%M:%S')] $*"
  echo "$line" >&2
  echo "$line" >> "$SUPERVISOR_LOG"
}

die() {
  log "ERROR: $*"
  exit 1
}

validate_int() {
  local value="$1"
  local name="$2"
  if ! [[ "$value" =~ ^[0-9]+$ ]]; then
    die "Invalid $name: $value (expected integer)"
  fi
}

trim() {
  local s="$1"
  s="${s#${s%%[![:space:]]*}}"
  s="${s%${s##*[![:space:]]}}"
  printf '%s' "$s"
}

run_with_timeout() {
  local seconds="$1"
  shift
  if command -v timeout >/dev/null 2>&1; then
    timeout "$seconds" "$@"
  elif command -v gtimeout >/dev/null 2>&1; then
    gtimeout "$seconds" "$@"
  else
    perl -e 'alarm shift @ARGV; exec @ARGV; die "exec failed: $!";' "$seconds" "$@"
  fi
}

validate_int "$MONITOR_INTERVAL" "monitor interval"
validate_int "$IDLE_RESTART_SEC" "idle restart seconds"
validate_int "$CLEANUP_INTERVAL_SEC" "cleanup interval seconds"
validate_int "$CLEANUP_STALE_DAYS" "cleanup stale days"
validate_int "$LOOP_TIMEOUT_SEC" "loop timeout seconds"
validate_int "$LOOP_SLEEP_SEC" "loop sleep seconds"
validate_int "$REPEAT_WINDOW_LINES" "repeat window lines"
validate_int "$REPEAT_THRESHOLD" "repeat threshold"
validate_int "$MANAGER_INTERVAL" "manager interval"
validate_int "$MANAGER_TIMEOUT" "manager timeout"
validate_int "$MODEL_PROBE_TIMEOUT" "model probe timeout"

case "$MODE" in
  conformance|emit|lsp|arch) ;;
  *) die "Unsupported mode: $MODE" ;;
esac

if ! command -v tmux >/dev/null 2>&1; then
  die "tmux not found in PATH"
fi
if ! command -v gemini >/dev/null 2>&1; then
  die "gemini not found in PATH"
fi
if [[ "$MANAGER_ENABLED" == true ]] && ! command -v jq >/dev/null 2>&1; then
  log "WARN: jq missing; disabling AI manager"
  MANAGER_ENABLED=false
fi

load_gemini_env() {
  local env_file="$HOME/.gemini/.env"
  if [[ -f "$env_file" ]]; then
    set -a
    # shellcheck disable=SC1090
    source "$env_file"
    set +a
    log "Loaded Gemini environment from $env_file"
  else
    log "WARN: $env_file not found; expecting env vars from shell"
  fi
}

normalize_vertex_env() {
  if [[ -z "${GOOGLE_API_KEY:-}" && -n "${GCP_VERTEX_EXPRESS_API_KEY:-}" ]]; then
    export GOOGLE_API_KEY="$GCP_VERTEX_EXPRESS_API_KEY"
    log "Mapped GCP_VERTEX_EXPRESS_API_KEY -> GOOGLE_API_KEY"
  fi
  if [[ -z "${GOOGLE_CLOUD_PROJECT:-}" && -n "${VERTEX_AI_PROJECT_ID:-}" ]]; then
    export GOOGLE_CLOUD_PROJECT="$VERTEX_AI_PROJECT_ID"
    log "Mapped VERTEX_AI_PROJECT_ID -> GOOGLE_CLOUD_PROJECT"
  fi
  if [[ -z "${GOOGLE_CLOUD_LOCATION:-}" && -n "${VERTEX_AI_LOCATION:-}" ]]; then
    export GOOGLE_CLOUD_LOCATION="$VERTEX_AI_LOCATION"
    log "Mapped VERTEX_AI_LOCATION -> GOOGLE_CLOUD_LOCATION"
  fi
  if [[ -z "${GOOGLE_GENAI_USE_VERTEXAI:-}" ]]; then
    export GOOGLE_GENAI_USE_VERTEXAI="true"
  fi
}

load_gemini_env
normalize_vertex_env

has_session() {
  tmux has-session -t "$SESSION_NAME" >/dev/null 2>&1
}

stop_session() {
  if has_session; then
    tmux kill-session -t "$SESSION_NAME"
    log "Stopped tmux session: $SESSION_NAME"
  else
    log "No tmux session named $SESSION_NAME"
  fi
}

probe_model() {
  local model="$1"
  local out
  local status

  set +e
  out="$(cd "$GEMINI_CONTROL_DIR" && run_with_timeout "$MODEL_PROBE_TIMEOUT" gemini --model "$model" -p "Reply with exactly: ok" -o json 2>>"$SUPERVISOR_LOG")"
  status=$?
  set -e

  if (( status != 0 )); then
    return 1
  fi

  if ! printf '%s' "$out" | jq -e '.response' >/dev/null 2>&1; then
    return 1
  fi

  return 0
}

resolve_model() {
  local requested="$1"
  local role="$2"
  local chosen=""

  if probe_model "$requested"; then
    chosen="$requested"
  else
    log "WARN: $role model not available: $requested"
    local IFS=','
    local candidate
    for candidate in $MODEL_CANDIDATES; do
      candidate="$(trim "$candidate")"
      [[ -n "$candidate" ]] || continue
      [[ "$candidate" == "$requested" ]] && continue
      if probe_model "$candidate"; then
        chosen="$candidate"
        break
      fi
    done
  fi

  if [[ -z "$chosen" ]]; then
    die "No usable model found for $role. Requested=$requested candidates=$MODEL_CANDIDATES"
  fi

  log "Using $role model: $chosen"
  printf '%s' "$chosen"
}

discover_repos() {
  local use_version_sort=false
  if sort -V </dev/null >/dev/null 2>&1; then
    use_version_sort=true
  fi

  REPOS=()
  if [[ "$use_version_sort" == true ]]; then
    while IFS= read -r d; do
      [[ -n "$d" ]] || continue
      [[ -f "$d/Cargo.toml" ]] || continue
      [[ -f "$d/scripts/gemini-loop.sh" ]] || continue
      REPOS+=("$d")
    done < <(find "$PARENT_DIR" -maxdepth 1 -type d -name "$REPO_GLOB" | sort -V)
  else
    while IFS= read -r d; do
      [[ -n "$d" ]] || continue
      [[ -f "$d/Cargo.toml" ]] || continue
      [[ -f "$d/scripts/gemini-loop.sh" ]] || continue
      REPOS+=("$d")
    done < <(find "$PARENT_DIR" -maxdepth 1 -type d -name "$REPO_GLOB" | sort)
  fi

  if [[ ${#REPOS[@]} -eq 0 ]]; then
    die "No repos found under $PARENT_DIR matching $REPO_GLOB"
  fi
}

ensure_main_branch_if_clean() {
  local repo="$1"
  local name
  local branch
  local dirty

  name="$(basename "$repo")"
  branch="$(git -C "$repo" branch --show-current 2>/dev/null || true)"
  [[ -n "$branch" ]] || return
  [[ "$branch" == "main" ]] && return

  dirty="$(git -C "$repo" status --porcelain 2>/dev/null || true)"
  if [[ -n "$dirty" ]]; then
    log "WARN: $name stays on $branch (has local changes)"
    return
  fi

  if git -C "$repo" checkout main >/dev/null 2>&1; then
    log "Switched $name from $branch to main"
  else
    log "WARN: failed to switch $name from $branch to main"
  fi
}

build_worker_cmd() {
  local repo="$1"
  local session_id="$2"
  local total_chunks="$3"

  local cmd=""
  cmd+="cd $(printf '%q' "$repo")"
  cmd+=" && export GEMINI_LOOP_CONFORMANCE_CHUNKS=$(printf '%q' "$total_chunks")"
  cmd+=" && export GEMINI_LOOP_TIMEOUT=$(printf '%q' "$LOOP_TIMEOUT_SEC")"
  cmd+=" && export GEMINI_LOOP_SLEEP=$(printf '%q' "$LOOP_SLEEP_SEC")"
  cmd+=" && ./scripts/gemini-loop.sh --$(printf '%q' "$MODE") --session $(printf '%q' "$session_id") --model $(printf '%q' "$WORKER_MODEL_RESOLVED")"
  if [[ -n "$PROMPT_FILE" ]]; then
    cmd+=" --prompt-file $(printf '%q' "$PROMPT_FILE")"
  fi
  printf '%s' "$cmd"
}

status_action() {
  if ! has_session; then
    echo "No session named $SESSION_NAME"
    exit 1
  fi

  echo "Session: $SESSION_NAME"
  tmux list-windows -t "$SESSION_NAME" -F 'window=#{window_index} name=#{window_name} dead=#{pane_dead} pid=#{pane_pid}'
}

attach_action() {
  if ! has_session; then
    die "No session named $SESSION_NAME"
  fi
  tmux attach -t "$SESSION_NAME"
}

if [[ "$ACTION" == "stop" ]]; then
  stop_session
  exit 0
fi
if [[ "$ACTION" == "status" ]]; then
  status_action
  exit 0
fi
if [[ "$ACTION" == "attach" ]]; then
  attach_action
  exit 0
fi
if has_session; then
  die "Session $SESSION_NAME already exists. Use stop/status/attach."
fi

WORKER_MODEL_RESOLVED="$(resolve_model "$WORKER_MODEL" "worker")"
if [[ -z "$MANAGER_MODEL" ]]; then
  MANAGER_MODEL="$WORKER_MODEL_RESOLVED"
fi
if [[ "$MANAGER_ENABLED" == true ]]; then
  MANAGER_MODEL_RESOLVED="$(resolve_model "$MANAGER_MODEL" "manager")"
else
  MANAGER_MODEL_RESOLVED="$MANAGER_MODEL"
fi

discover_repos
TOTAL_REPOS="${#REPOS[@]}"

log "Starting Gemini fleet session=$SESSION_NAME mode=$MODE repos=$TOTAL_REPOS"
log "Worker model=$WORKER_MODEL_RESOLVED manager_enabled=$MANAGER_ENABLED manager_model=$MANAGER_MODEL_RESOLVED"
log "Repo glob: $REPO_GLOB"
log "Ctrl+C stops all workers and kills session"
log "Supervisor log: $SUPERVISOR_LOG"

for repo in "${REPOS[@]}"; do
  ensure_main_branch_if_clean "$repo"
done

WINDOW_NAMES=()
WORKER_CMDS=()
WORKER_LOGS=()
RESTART_COUNTS=()
PANE_DEAD_STATE=()
LOG_AGE_STATE=()
REPEAT_STATE=()

for ((i=0; i<TOTAL_REPOS; i++)); do
  repo="${REPOS[$i]}"
  session_id=$((i + 1))
  name="$(basename "$repo")"
  pane_log="$FLEET_LOG_DIR/$(printf '%02d' "$session_id")-$name.tmux.log"
  worker_log="$repo/logs/gemini-loop.session-${session_id}.${MODE}.log"
  cmd="$(build_worker_cmd "$repo" "$session_id" "$TOTAL_REPOS")"

  WINDOW_NAMES+=("$name")
  WORKER_CMDS+=("$cmd")
  WORKER_LOGS+=("$worker_log")
  RESTART_COUNTS+=(0)
  PANE_DEAD_STATE+=(0)
  LOG_AGE_STATE+=(0)
  REPEAT_STATE+=(0)

  if [[ "$i" -eq 0 ]]; then
    tmux new-session -d -s "$SESSION_NAME" -n "$name" "$cmd"
  else
    tmux new-window -d -t "$SESSION_NAME" -n "$name" "$cmd"
  fi

  tmux pipe-pane -o -t "${SESSION_NAME}:${i}.0" "cat >> '$pane_log'"
  log "Launched [$session_id/$TOTAL_REPOS] $name"
done

cleanup_and_exit() {
  log "Stopping fleet due to signal"
  stop_session
  exit 0
}
trap cleanup_and_exit INT TERM

run_cleanup_once() {
  if [[ "$RUN_CLEANUP" != true ]]; then
    return
  fi
  log "Maintenance: cleanup-build-artifacts --others --stale $CLEANUP_STALE_DAYS"
  "$REPO_ROOT/scripts/cleanup-build-artifacts.sh" --others --stale "$CLEANUP_STALE_DAYS" >> "$SUPERVISOR_LOG" 2>&1 || true
}

run_runaway_guard_once() {
  if [[ "$RUN_RUNAWAY_GUARD" != true ]]; then
    return
  fi

  local repo
  for repo in "${REPOS[@]}"; do
    if [[ -x "$repo/scripts/kill-runaway-processes.sh" ]]; then
      "$repo/scripts/kill-runaway-processes.sh" --check >> "$SUPERVISOR_LOG" 2>&1 || true
    fi
  done
  log "Maintenance: runaway process check complete"
}

repeat_line_count() {
  local file="$1"
  if [[ ! -f "$file" ]]; then
    echo 0
    return
  fi

  tail -n "$REPEAT_WINDOW_LINES" "$file" 2>/dev/null \
    | sed -E 's/^\[[0-9TZ:+ -]+\] //g' \
    | sed '/^[[:space:]]*$/d' \
    | sort \
    | uniq -c \
    | sort -nr \
    | head -n 1 \
    | awk '{print $1+0}'
}

restart_worker() {
  local idx="$1"
  local reason="$2"
  local id=$((idx + 1))
  local name="${WINDOW_NAMES[$idx]}"
  local pane_log="$FLEET_LOG_DIR/$(printf '%02d' "$id")-$name.tmux.log"
  local cmd="${WORKER_CMDS[$idx]}"
  local count="${RESTART_COUNTS[$idx]}"

  tmux respawn-pane -k -t "${SESSION_NAME}:${idx}.0" "$cmd"
  tmux pipe-pane -o -t "${SESSION_NAME}:${idx}.0" "cat >> '$pane_log'"

  count=$((count + 1))
  RESTART_COUNTS[$idx]="$count"
  log "Restarted worker $name (#$id) reason=$reason restart_count=$count"
}

run_manager_decision() {
  local now="$1"

  if [[ "$MANAGER_ENABLED" != true ]]; then
    return
  fi

  local snapshot=""
  local i
  for ((i=0; i<TOTAL_REPOS; i++)); do
    snapshot+=$'\n'
    snapshot+="id=$((i+1))"
    snapshot+=" name=${WINDOW_NAMES[$i]}"
    snapshot+=" pane_dead=${PANE_DEAD_STATE[$i]}"
    snapshot+=" log_age_sec=${LOG_AGE_STATE[$i]}"
    snapshot+=" repeat_count=${REPEAT_STATE[$i]}"
    snapshot+=" restarts=${RESTART_COUNTS[$i]}"
  done

  local prompt
  prompt=$(cat <<AI_PROMPT
You are supervising a fleet of long-running Gemini workers in tmux.
Choose minimal actions that maximize productive progress and avoid churn.

Return STRICT JSON ONLY:
{"restart_window_ids":[1],"run_cleanup":false,"run_runaway_guard":false,"notes":"short reason"}

Rules:
- Prefer no action when healthy.
- Restart only clearly stuck workers.
- Never restart more than 25% of workers in one decision.
- Use run_cleanup=true only when many workers look idle or when disk pressure is likely.
- Use run_runaway_guard=true only when workers appear CPU-stuck for too long.

Context:
mode=$MODE total_workers=$TOTAL_REPOS monitor_interval_sec=$MONITOR_INTERVAL
idle_restart_sec=$IDLE_RESTART_SEC repeat_threshold=$REPEAT_THRESHOLD
workers:${snapshot}
AI_PROMPT
)

  local raw
  local status
  set +e
  raw="$(cd "$GEMINI_CONTROL_DIR" && run_with_timeout "$MANAGER_TIMEOUT" gemini --model "$MANAGER_MODEL_RESOLVED" -p "$prompt" -o json 2>>"$SUPERVISOR_LOG")"
  status=$?
  set -e
  if (( status != 0 )); then
    log "AI manager skipped: call failed status=$status"
    return
  fi

  local decision_text
  decision_text="$(printf '%s' "$raw" | jq -r '.response // empty' 2>/dev/null || true)"
  decision_text="$(printf '%s\n' "$decision_text" | sed -E '/^```(json)?[[:space:]]*$/d')"

  if ! printf '%s' "$decision_text" | jq -e . >/dev/null 2>&1; then
    log "AI manager skipped: invalid JSON response"
    return
  fi

  local max_restarts=$(( (TOTAL_REPOS + 3) / 4 ))
  local applied=0
  local id
  while IFS= read -r id; do
    [[ -n "$id" ]] || continue
    if ! [[ "$id" =~ ^[0-9]+$ ]]; then
      continue
    fi
    local idx=$((id - 1))
    if (( idx < 0 || idx >= TOTAL_REPOS )); then
      continue
    fi
    if (( applied >= max_restarts )); then
      break
    fi
    restart_worker "$idx" "ai_manager"
    applied=$((applied + 1))
  done < <(printf '%s' "$decision_text" | jq -r '.restart_window_ids[]? // empty' 2>/dev/null || true)

  local ai_cleanup
  local ai_runaway
  local ai_notes
  ai_cleanup="$(printf '%s' "$decision_text" | jq -r '.run_cleanup // false' 2>/dev/null || echo false)"
  ai_runaway="$(printf '%s' "$decision_text" | jq -r '.run_runaway_guard // false' 2>/dev/null || echo false)"
  ai_notes="$(printf '%s' "$decision_text" | jq -r '.notes // ""' 2>/dev/null || true)"

  if [[ "$ai_cleanup" == "true" ]]; then
    run_cleanup_once
    next_cleanup_at=$((now + CLEANUP_INTERVAL_SEC))
  fi
  if [[ "$ai_runaway" == "true" ]]; then
    run_runaway_guard_once
  fi

  if [[ -n "$ai_notes" ]]; then
    log "AI manager note: $ai_notes"
  fi
}

next_cleanup_at=$(( $(date +%s) + CLEANUP_INTERVAL_SEC ))
next_manager_at=$(( $(date +%s) + MANAGER_INTERVAL ))

while true; do
  sleep "$MONITOR_INTERVAL"
  now="$(date +%s)"

  alive=0
  dead=0
  for ((i=0; i<TOTAL_REPOS; i++)); do
    pane_dead="$(tmux display-message -p -t "${SESSION_NAME}:${i}.0" "#{pane_dead}" 2>/dev/null || echo 1)"
    PANE_DEAD_STATE[$i]="$pane_dead"

    if [[ "$pane_dead" == "1" ]]; then
      dead=$((dead + 1))
      restart_worker "$i" "pane_dead"
      continue
    fi

    alive=$((alive + 1))

    worker_log="${WORKER_LOGS[$i]}"
    age=999999
    repeat_count=0
    if [[ -f "$worker_log" ]]; then
      last_mod="$(stat -f %m "$worker_log" 2>/dev/null || echo 0)"
      age=$((now - last_mod))
      repeat_count="$(repeat_line_count "$worker_log")"
    fi

    LOG_AGE_STATE[$i]="$age"
    REPEAT_STATE[$i]="$repeat_count"

    if (( age > IDLE_RESTART_SEC )); then
      restart_worker "$i" "idle_log_${age}s"
      continue
    fi
    if (( repeat_count >= REPEAT_THRESHOLD )); then
      restart_worker "$i" "repeat_line_count_${repeat_count}"
      continue
    fi
  done

  remaining_cleanup=$((next_cleanup_at - now))
  if (( remaining_cleanup < 0 )); then
    remaining_cleanup=0
  fi

  if [[ "$MANAGER_ENABLED" == true ]] && (( now >= next_manager_at )); then
    run_manager_decision "$now"
    next_manager_at=$((now + MANAGER_INTERVAL))
  fi

  if (( now >= next_cleanup_at )); then
    run_cleanup_once
    run_runaway_guard_once
    next_cleanup_at=$((now + CLEANUP_INTERVAL_SEC))
  fi

  total_restarts=0
  for count in "${RESTART_COUNTS[@]}"; do
    total_restarts=$((total_restarts + count))
  done

  log "Heartbeat alive=$alive dead=$dead restarts_total=$total_restarts next_cleanup_in=${remaining_cleanup}s"
done
