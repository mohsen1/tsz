#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/gemini-loop.sh [--conformance|--emit|--lsp|--arch]
                        [--session N|--session=N] [--prompt-file FILE]
                        [--workdir DIR] [--model MODEL] [--help]

Modes:
  --conformance   Continuous conformance loop (default)
  --emit          Continuous emitter loop
  --lsp           Continuous LSP loop
  --arch          Continuous architecture loop

Options:
  --session N     Session id for parallel loops
  --prompt-file   Override prompt file path
  --workdir DIR   Root directory to run gemini from (default: repo root)
  --model MODEL   Override model passed to `gemini --model`
  -h, --help      Show help
EOF
}

MODE="conformance"
SESSION_ID=""
MODEL_OVERRIDE=""
PROMPT_FILE=""
WORKDIR="$(pwd)"
SLEEP_SECONDS="${GEMINI_LOOP_SLEEP:-5}"
CONF_CHUNKS="${GEMINI_LOOP_CONFORMANCE_CHUNKS:-}"
CONF_TOTAL_TESTS="${GEMINI_LOOP_CONFORMANCE_TOTAL_TESTS:-12584}"
PROJECT_DIRECTIVES="${GEMINI_LOOP_PROJECT_DIRECTIVES:-Use tracing infrastructure (spans/events) instead of ad-hoc print debugging. This is a performance-first project: prioritize throughput, latency, and memory, and avoid regressions. The project is pre-release, so no backward compatibility constraints are required; prefer cleaner, faster designs over compatibility shims.}"
LOG_ROOT="${GEMINI_LOOP_LOG_ROOT:-logs/loops/gemini}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --conformance|--emit|--lsp|--arch)
      MODE="${1#--}"
      shift
      ;;
    --session)
      SESSION_ID="${2:-}"
      shift 2
      ;;
    --session=*)
      SESSION_ID="${1#*=}"
      shift
      ;;
    --prompt-file)
      PROMPT_FILE="${2:-}"
      shift 2
      ;;
    --workdir)
      WORKDIR="${2:-}"
      shift 2
      ;;
    --model)
      MODEL_OVERRIDE="${2:-}"
      shift 2
      ;;
    --model=*)
      MODEL_OVERRIDE="${1#*=}"
      shift
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

if [[ -n "$WORKDIR" && "$WORKDIR" == "~/"* ]]; then
  WORKDIR="${HOME}/${WORKDIR#~/}"
fi
if [[ -d "$WORKDIR" ]]; then
  WORKDIR="$(cd "$WORKDIR" && pwd)"
else
  echo "Workdir not found: $WORKDIR" >&2
  exit 1
fi

# Auto-detect session ID and total chunks if not provided
if [[ -z "$SESSION_ID" ]]; then
  # Try to detect if we are in a sharded environment (tsz-*)
  PARENT_DIR="$(dirname "$WORKDIR")"
  BASE_NAME="$(basename "$WORKDIR")"
  
  if [[ "$BASE_NAME" =~ ^tsz-[0-9]+$ ]]; then
    # Find all sibling directories matching tsz-[0-9]+
    # Use find to list, sort -V to order naturally
    # Note: maxdepth 1 to avoid recursion
    if command -v sort >/dev/null 2>&1 && sort -V </dev/null >/dev/null 2>&1; then
       SORT_CMD="sort -V"
    else
       # Fallback to standard sort (lexicographical) if -V is not supported
       SORT_CMD="sort" 
    fi

    # Read into array (bash 3.2+ safe)
    IFS=$'\n' read -r -d '' -a SIBLINGS < <(find "$PARENT_DIR" -maxdepth 1 -type d -name "tsz-[0-9]*" -exec basename {} \; | $SORT_CMD && printf '\0')
    
    TOTAL_SHARDS=${#SIBLINGS[@]}
    MY_INDEX=-1
    
    for i in "${!SIBLINGS[@]}"; do
      if [[ "${SIBLINGS[$i]}" == "$BASE_NAME" ]]; then
        MY_INDEX=$i
        break
      fi
    done
    
    if [[ $MY_INDEX -ge 0 ]]; then
      SESSION_ID=$((MY_INDEX + 1))
      # Only set CONF_CHUNKS if not already set by env var
      if [[ -z "$CONF_CHUNKS" ]]; then
        CONF_CHUNKS=$TOTAL_SHARDS
      fi
      echo "Auto-detected sharding: Session $SESSION_ID of $CONF_CHUNKS (based on $BASE_NAME among $TOTAL_SHARDS siblings)"
    fi
  fi
fi

# Default CONF_CHUNKS if still unset
if [[ -z "$CONF_CHUNKS" ]]; then
  CONF_CHUNKS=4
fi

if [[ -n "$SESSION_ID" ]] && ! [[ "$SESSION_ID" =~ ^[0-9]+$ ]]; then
  echo "Invalid session id: $SESSION_ID (expected integer)" >&2
  exit 1
fi

GEMINI_BIN="gemini"
if ! command -v "$GEMINI_BIN" >/dev/null 2>&1; then
  echo "gemini CLI not found in PATH" >&2
  exit 1
fi

case "$MODE" in
  conformance) PROMPT_FILE="${PROMPT_FILE:-scripts/gemini-loop.prompt.conformance.txt}" ;;
  emit)        PROMPT_FILE="${PROMPT_FILE:-scripts/gemini-loop.prompt.emit.txt}" ;;
  lsp)         PROMPT_FILE="${PROMPT_FILE:-scripts/gemini-loop.prompt.lsp.txt}" ;;
  arch)        PROMPT_FILE="${PROMPT_FILE:-scripts/gemini-loop.prompt.arch.txt}" ;;
  *) echo "Unsupported mode: $MODE" >&2; exit 1 ;;
esac

if [[ ! -f "$PROMPT_FILE" ]]; then
  # Fallback to codex prompts if gemini ones don't exist, but warn
  CODEX_PROMPT_FILE="${PROMPT_FILE/gemini/codex}"
  if [[ -f "$CODEX_PROMPT_FILE" ]]; then
    echo "Warning: $PROMPT_FILE not found, using $CODEX_PROMPT_FILE" >&2
    PROMPT_FILE="$CODEX_PROMPT_FILE"
  else
    echo "Prompt file not found: $PROMPT_FILE" >&2
    exit 1
  fi
fi

MODEL="${MODEL_OVERRIDE:-gemini-3.1-pro-preview}"

mkdir -p "$LOG_ROOT"
if [[ -n "$SESSION_ID" ]]; then
  LOG_FILE="${LOG_ROOT}/session-${SESSION_ID}.${MODE}.log"
  SESSION_TAG=" session=$SESSION_ID"
else
  LOG_FILE="${LOG_ROOT}/${MODE}.log"
  SESSION_TAG=""
fi

if ! git -C "$WORKDIR" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "Workdir is not a git repository: $WORKDIR" >&2
  exit 1
fi

verify_iteration_completion() {
  local repo="$1"
  local start_head="$2"
  local iter="$3"
  local end_head commit_delta

  if ! end_head="$(git -C "$repo" rev-parse HEAD 2>/dev/null)"; then
    echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] iteration=$iter completion_gate=failed reason=head_unavailable" | tee -a "$LOG_FILE"
    return 41
  fi

  if [[ "$end_head" == "$start_head" ]]; then
    echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] iteration=$iter completion_gate=failed reason=no_new_commit" | tee -a "$LOG_FILE"
    return 42
  fi

  commit_delta="$(git -C "$repo" rev-list --count "${start_head}..${end_head}" 2>/dev/null || echo 0)"
  if [[ "$commit_delta" != "1" ]]; then
    echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] iteration=$iter completion_gate=warn detail=expected_one_commit_observed_$commit_delta" | tee -a "$LOG_FILE"
  fi

  if ! git -C "$repo" diff --quiet || ! git -C "$repo" diff --cached --quiet; then
    echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] iteration=$iter completion_gate=failed reason=dirty_worktree" | tee -a "$LOG_FILE"
    return 43
  fi

  if ! git -C "$repo" fetch --quiet origin main >/dev/null 2>&1; then
    echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] iteration=$iter completion_gate=failed reason=fetch_origin_main_failed" | tee -a "$LOG_FILE"
    return 44
  fi

  if ! git -C "$repo" merge-base --is-ancestor "$end_head" origin/main; then
    echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] iteration=$iter completion_gate=failed reason=commit_not_on_origin_main commit=$end_head" | tee -a "$LOG_FILE"
    return 45
  fi

  echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] iteration=$iter completion_gate=passed commit=$end_head" | tee -a "$LOG_FILE"
  return 0
}

build_prompt() {
  local prompt
  prompt="$(cat "$PROMPT_FILE")"
  if [[ -n "$SESSION_ID" ]]; then
    prompt="Session ${SESSION_ID}: ${prompt}"
  fi

  if [[ "$MODE" == "conformance" && -n "$SESSION_ID" ]]; then
    local shard_index shard_label shard_size shard_offset remaining shard_max
    shard_index=$(( (SESSION_ID - 1) % CONF_CHUNKS ))
    shard_label=$(( shard_index + 1 ))
    shard_size=$(( (CONF_TOTAL_TESTS + CONF_CHUNKS - 1) / CONF_CHUNKS ))
    shard_offset=$(( shard_index * shard_size ))
    remaining=$(( CONF_TOTAL_TESTS - shard_offset ))
    shard_max="$shard_size"
    if (( remaining < shard_max )); then
      shard_max=$remaining
    fi
    prompt="${prompt} Parallel conformance sharding: you own chunk ${shard_label}/${CONF_CHUNKS}. Focus your test slice with scripts/conformance.sh analyze --offset ${shard_offset} --max ${shard_max}."
  fi

  prompt="${prompt} Global project directives: ${PROJECT_DIRECTIVES}"

  printf '%s
' "$prompt"
}

echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] starting mode=$MODE${SESSION_TAG} workdir=$WORKDIR log=$LOG_FILE model=$MODEL" | tee -a "$LOG_FILE"

iteration=0
while true; do
  iteration=$((iteration + 1))
  echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] mode=$MODE${SESSION_TAG} iteration=$iteration" | tee -a "$LOG_FILE"

  ITERATION_START_HEAD="$(git -C "$WORKDIR" rev-parse HEAD 2>/dev/null || true)"
  PROMPT_TEXT="$(build_prompt)"
  
  # Construct command
  CMD=( "$GEMINI_BIN" --yolo --model "$MODEL" --prompt "$PROMPT_TEXT" )

  # Retry loop for transient failures (e.g. ModelNotFound or timeouts)
  MAX_RETRIES=3
  retry_count=0
  
  while (( retry_count < MAX_RETRIES )); do
    set +e
    "${CMD[@]}" 2>&1 | tee -a "$LOG_FILE"
    status=${PIPESTATUS[0]:-0}
    set -e
    
    # Check for specific success or failure patterns
    if (( status == 0 )); then
       break
    fi
    
    # If we see ModelNotFoundError, we could try to fallback, but for now just retry/log
    # You might want to grep the log for "ModelNotFoundError" to decide if you want to switch models dynamically
    
    echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] iteration=$iteration attempt=$((retry_count+1)) failed with status=$status. Retrying..." | tee -a "$LOG_FILE"
    sleep "$SLEEP_SECONDS"
    retry_count=$((retry_count + 1))
  done

  if (( status == 0 )); then
    if [[ -z "$ITERATION_START_HEAD" ]]; then
      echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] iteration=$iteration completion_gate=failed reason=start_head_unavailable" | tee -a "$LOG_FILE"
      status=40
    elif ! verify_iteration_completion "$WORKDIR" "$ITERATION_START_HEAD" "$iteration"; then
      status=$?
    fi
  fi

  echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] iteration=$iteration final_exit_status=$status" | tee -a "$LOG_FILE"

  # Clean up dirty worktree between iterations so the next one starts fresh
  if ! git -C "$WORKDIR" diff --quiet 2>/dev/null || ! git -C "$WORKDIR" diff --cached --quiet 2>/dev/null; then
    echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] iteration=$iteration cleanup=resetting_dirty_worktree" | tee -a "$LOG_FILE"
    # Salvage uncommitted work as a patch before resetting
    SALVAGE_DIR="${LOG_ROOT}/salvage"
    mkdir -p "$SALVAGE_DIR"
    PATCH_FILE="${SALVAGE_DIR}/session-${SESSION_ID:-0}-iter-${iteration}-$(date -u +%Y%m%dT%H%M%SZ).patch"
    {
      git -C "$WORKDIR" diff 2>/dev/null
      git -C "$WORKDIR" diff --cached 2>/dev/null
    } > "$PATCH_FILE"
    if [[ -s "$PATCH_FILE" ]]; then
      echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] iteration=$iteration salvaged_patch=$PATCH_FILE" | tee -a "$LOG_FILE"
    else
      rm -f "$PATCH_FILE"
    fi
    git -C "$WORKDIR" reset --hard HEAD >/dev/null 2>&1
    git -C "$WORKDIR" clean -fd >/dev/null 2>&1
  fi

  # Sync to latest origin/main before next iteration
  if git -C "$WORKDIR" fetch --quiet origin main >/dev/null 2>&1; then
    git -C "$WORKDIR" rebase origin/main >/dev/null 2>&1 || git -C "$WORKDIR" reset --hard origin/main >/dev/null 2>&1
  fi

  sleep "$SLEEP_SECONDS"
done
