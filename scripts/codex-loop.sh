#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/codex-loop.sh [--conformance|--emit|--lsp|--arch|--spark]
                        [--session N|--session=N] [--prompt-file FILE]
                        [--workdir DIR] [--model MODEL]
                        [--sandbox MODE] [--approval POLICY] [--bypass]
                        [--help]

Modes:
  --conformance   Continuous conformance loop (default)
  --emit          Continuous emitter loop
  --lsp           Continuous LSP loop
  --arch          Continuous architecture loop
  --spark         Same as conformance but uses the spark model

Options:
  --session N     Session id for parallel loops (also used for conformance sharding)
  --prompt-file   Override prompt file path
  --workdir DIR   Root directory to run codex from (default: repo root)
  --model MODEL   Override model passed to `codex exec --model`
  --sandbox MODE  Codex sandbox mode: read-only|workspace-write|danger-full-access
                  (default: danger-full-access)
  --approval POL  Codex approval policy: untrusted|on-failure|on-request|never (default: never)
  --bypass        Pass --dangerously-bypass-approvals-and-sandbox to codex exec
  -h, --help      Show help
EOF
}

MODE="conformance"
SESSION_ID=""
MODEL_OVERRIDE=""
PROMPT_FILE=""
WORKDIR="$(pwd)"
SANDBOX_MODE="${CODEX_LOOP_SANDBOX:-danger-full-access}"
APPROVAL_POLICY="${CODEX_LOOP_APPROVAL:-never}"
BYPASS_APPROVALS_AND_SANDBOX="${CODEX_LOOP_BYPASS:-0}"
TIMEOUT_SECONDS="${CODEX_LOOP_TIMEOUT:-900}"
SLEEP_SECONDS="${CODEX_LOOP_SLEEP:-2}"
CONF_QUARTERS="${CODEX_LOOP_CONFORMANCE_QUARTERS:-}"
CONF_TOTAL_TESTS="${CODEX_LOOP_CONFORMANCE_TOTAL_TESTS:-12584}"
LOG_ROOT="${CODEX_LOOP_LOG_ROOT:-logs/loops/codex}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --conformance|--emit|--lsp|--arch|--spark)
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
    --sandbox)
      SANDBOX_MODE="${2:-}"
      shift 2
      ;;
    --sandbox=*)
      SANDBOX_MODE="${1#*=}"
      shift
      ;;
    --approval)
      APPROVAL_POLICY="${2:-}"
      shift 2
      ;;
    --approval=*)
      APPROVAL_POLICY="${1#*=}"
      shift
      ;;
    --bypass)
      BYPASS_APPROVALS_AND_SANDBOX=1
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

if [[ -n "$SESSION_ID" ]] && ! [[ "$SESSION_ID" =~ ^[0-9]+$ ]]; then
  echo "Invalid session id: $SESSION_ID (expected integer)" >&2
  exit 1
fi

parent_dir="$(dirname "$WORKDIR")"
base_name="$(basename "$WORKDIR")"
if [[ "$base_name" =~ ^tsz-[0-9]+$ ]]; then
  if command -v sort >/dev/null 2>&1 && sort -V </dev/null >/dev/null 2>&1; then
    sort_cmd="sort -V"
  else
    sort_cmd="sort"
  fi

  IFS=$'\n' read -r -d '' -a siblings < <(find "$parent_dir" -maxdepth 1 -type d -name "tsz-[0-9]*" -exec basename {} \; | $sort_cmd && printf '\0')
  total_shards=${#siblings[@]}
  my_index=-1
  for i in "${!siblings[@]}"; do
    if [[ "${siblings[$i]}" == "$base_name" ]]; then
      my_index=$i
      break
    fi
  done
  if [[ -z "$CONF_QUARTERS" && "$total_shards" -gt 0 ]]; then
    CONF_QUARTERS="$total_shards"
  fi
  if [[ -z "$SESSION_ID" && $my_index -ge 0 ]]; then
    SESSION_ID=$((my_index + 1))
  fi
  if [[ $my_index -ge 0 ]]; then
    echo "Auto-detected sharding: Session ${SESSION_ID:-$((my_index + 1))} of ${CONF_QUARTERS:-$total_shards} (based on $base_name among $total_shards siblings)"
  fi
fi

if [[ -z "$CONF_QUARTERS" ]]; then
  CONF_QUARTERS=4
fi

case "$SANDBOX_MODE" in
  read-only|workspace-write|danger-full-access) ;;
  *)
    echo "Invalid sandbox mode: $SANDBOX_MODE" >&2
    exit 1
    ;;
esac

case "$APPROVAL_POLICY" in
  untrusted|on-failure|on-request|never) ;;
  *)
    echo "Invalid approval policy: $APPROVAL_POLICY" >&2
    exit 1
    ;;
esac

if ! command -v codex >/dev/null 2>&1; then
  echo "codex CLI not found in PATH" >&2
  exit 1
fi
CODEX_BIN="$(command -v codex)"

case "$MODE" in
  conformance|spark) PROMPT_FILE="${PROMPT_FILE:-scripts/codex-loop.prompt.conformance.txt}" ;;
  emit)              PROMPT_FILE="${PROMPT_FILE:-scripts/codex-loop.prompt.emit.txt}" ;;
  lsp)               PROMPT_FILE="${PROMPT_FILE:-scripts/codex-loop.prompt.lsp.txt}" ;;
  arch)              PROMPT_FILE="${PROMPT_FILE:-scripts/codex-loop.prompt.arch.txt}" ;;
  *) echo "Unsupported mode: $MODE" >&2; exit 1 ;;
esac

if [[ ! -f "$PROMPT_FILE" ]]; then
  echo "Prompt file not found: $PROMPT_FILE" >&2
  exit 1
fi

MODEL="${MODEL_OVERRIDE:-gpt-5.3-codex}"
if [[ "$MODE" == "spark" && -z "$MODEL_OVERRIDE" ]]; then
  MODEL="${CODEX_LOOP_SPARK_MODEL:-${MODEL}}"
fi

if printf '%s\n' "$MODEL" | tr '[:upper:]' '[:lower:]' | grep -q 'spark'; then
  REASONING_EFFORT="xhigh"
else
  # gpt-5.3-codex rejects minimal in some CLI/API combinations; low is safe.
  REASONING_EFFORT="low"
fi

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
    shard_index=$(( (SESSION_ID - 1) % CONF_QUARTERS ))
    shard_label=$(( shard_index + 1 ))
    shard_size=$(( (CONF_TOTAL_TESTS + CONF_QUARTERS - 1) / CONF_QUARTERS ))
    shard_offset=$(( shard_index * shard_size ))
    remaining=$(( CONF_TOTAL_TESTS - shard_offset ))
    shard_max="$shard_size"
    if (( remaining < shard_max )); then
      shard_max=$remaining
    fi
    prompt="${prompt} Parallel conformance sharding: you own quarter ${shard_label}/${CONF_QUARTERS}. Focus your test slice with scripts/conformance.sh analyze --offset ${shard_offset} --max ${shard_max}."
  fi

  prompt="${prompt}

Mandatory completion gate for this iteration:
1) End the iteration with exactly one commit.
2) Sync and push that commit to main using:
   git pull --rebase origin main
   git push origin HEAD:main
3) If there are no file changes, create an explicit empty checkpoint commit and push it:
   git commit --allow-empty -m \"chore(loop): iteration checkpoint\""

  printf '%s\n' "$prompt"
}

echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] starting mode=$MODE${SESSION_TAG} workdir=$WORKDIR log=$LOG_FILE model=$MODEL reasoning=$REASONING_EFFORT sandbox=$SANDBOX_MODE approval=$APPROVAL_POLICY bypass=$BYPASS_APPROVALS_AND_SANDBOX" | tee -a "$LOG_FILE"

iteration=0
while true; do
  iteration=$((iteration + 1))
  echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] mode=$MODE${SESSION_TAG} iteration=$iteration" | tee -a "$LOG_FILE"

  ITERATION_START_HEAD="$(git -C "$WORKDIR" rev-parse HEAD 2>/dev/null || true)"
  PROMPT_TEXT="$(build_prompt)"
  CMD=( "$CODEX_BIN" exec --model "$MODEL" -s "$SANDBOX_MODE" -c "approval_policy=\"$APPROVAL_POLICY\"" -c "model_reasoning_effort=$REASONING_EFFORT" -C "$WORKDIR" )
  if [[ "$BYPASS_APPROVALS_AND_SANDBOX" == "1" ]]; then
    CMD+=( --dangerously-bypass-approvals-and-sandbox )
  fi
  # The OpenAI Responses API rejects web_search with minimal reasoning effort.
  if [[ "$REASONING_EFFORT" == "minimal" ]]; then
    CMD+=( -c 'web_search="disabled"' )
  fi

  set +e
  if command -v timeout >/dev/null 2>&1; then
    timeout "$TIMEOUT_SECONDS" "${CMD[@]}" "$PROMPT_TEXT" 2>&1 \
      | sed '/state db missing rollout path for thread/d' \
      | tee -a "$LOG_FILE"
  else
    "${CMD[@]}" "$PROMPT_TEXT" 2>&1 \
      | sed '/state db missing rollout path for thread/d' \
      | tee -a "$LOG_FILE"
  fi
  status=${PIPESTATUS[0]:-0}
  set -e

  if (( status == 0 )); then
    if [[ -z "$ITERATION_START_HEAD" ]]; then
      echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] iteration=$iteration completion_gate=failed reason=start_head_unavailable" | tee -a "$LOG_FILE"
      status=40
    else
      completion_status=0
      verify_iteration_completion "$WORKDIR" "$ITERATION_START_HEAD" "$iteration" || completion_status=$?
      if (( completion_status != 0 )); then
        status=$completion_status
      fi
    fi
  fi

  echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] iteration=$iteration exit_status=$status" | tee -a "$LOG_FILE"

  if (( status != 0 )); then
    if [[ "$MODE" == "spark" && "$status" -ne 0 ]]; then
      echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] spark mode failed with status=$status; exiting" | tee -a "$LOG_FILE"
      exit "$status"
    fi
  fi

  sleep "$SLEEP_SECONDS"
done
