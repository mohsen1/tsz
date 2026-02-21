#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/claude-loop.sh [--conformance|--emit|--lsp|--arch]
                         [--session N|--session=N] [--prompt-file FILE]
                         [--workdir DIR] [--claude-command CMD] [--help]

Modes:
  --conformance   Continuous conformance loop (default)
  --emit          Continuous emitter loop
  --lsp           Continuous LSP loop
  --arch          Continuous architecture loop

Options:
  --session N          Session id for parallel loops
  --prompt-file FILE   Override prompt file path
  --workdir DIR        Root directory to run Claude from (default: repo root)
  --claude-command CMD Claude command/alias to execute (default: "claude --print")
  -h, --help           Show help

Command placeholders:
  {prompt}   Insert shell-escaped prompt text
  {workdir}  Insert shell-escaped workdir

If {prompt} is not present, the prompt is appended as the final command argument.
EOF
}

MODE="conformance"
SESSION_ID=""
PROMPT_FILE=""
WORKDIR="$(pwd)"
CLAUDE_COMMAND="${CLAUDE_LOOP_COMMAND:-claude --print}"
TIMEOUT_SECONDS="${CLAUDE_LOOP_TIMEOUT:-300}"
SLEEP_SECONDS="${CLAUDE_LOOP_SLEEP:-5}"
CONF_CHUNKS="${CLAUDE_LOOP_CONFORMANCE_CHUNKS:-4}"
CONF_TOTAL_TESTS="${CLAUDE_LOOP_CONFORMANCE_TOTAL_TESTS:-12584}"
SHELL_BIN="${CLAUDE_LOOP_SHELL:-/bin/zsh}"
LOG_ROOT="${CLAUDE_LOOP_LOG_ROOT:-logs/loops/claude}"

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
    --claude-command)
      CLAUDE_COMMAND="${2:-}"
      shift 2
      ;;
    --claude-command=*)
      CLAUDE_COMMAND="${1#*=}"
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

if [[ -z "$CLAUDE_COMMAND" ]]; then
  echo "--claude-command cannot be empty" >&2
  exit 1
fi

case "$MODE" in
  conformance) PROMPT_FILE="${PROMPT_FILE:-scripts/claude-loop.prompt.conformance.txt}" ;;
  emit)        PROMPT_FILE="${PROMPT_FILE:-scripts/claude-loop.prompt.emit.txt}" ;;
  lsp)         PROMPT_FILE="${PROMPT_FILE:-scripts/claude-loop.prompt.lsp.txt}" ;;
  arch)        PROMPT_FILE="${PROMPT_FILE:-scripts/claude-loop.prompt.arch.txt}" ;;
  *) echo "Unsupported mode: $MODE" >&2; exit 1 ;;
esac

if [[ ! -f "$PROMPT_FILE" ]]; then
  CODEX_PROMPT_FILE="${PROMPT_FILE/claude/codex}"
  GEMINI_PROMPT_FILE="${PROMPT_FILE/claude/gemini}"
  if [[ -f "$CODEX_PROMPT_FILE" ]]; then
    echo "Warning: $PROMPT_FILE not found, using $CODEX_PROMPT_FILE" >&2
    PROMPT_FILE="$CODEX_PROMPT_FILE"
  elif [[ -f "$GEMINI_PROMPT_FILE" ]]; then
    echo "Warning: $PROMPT_FILE not found, using $GEMINI_PROMPT_FILE" >&2
    PROMPT_FILE="$GEMINI_PROMPT_FILE"
  else
    echo "Prompt file not found: $PROMPT_FILE" >&2
    exit 1
  fi
fi

mkdir -p "$LOG_ROOT"
if [[ -n "$SESSION_ID" ]]; then
  LOG_FILE="${LOG_ROOT}/session-${SESSION_ID}.${MODE}.log"
  SESSION_TAG=" session=$SESSION_ID"
else
  LOG_FILE="${LOG_ROOT}/${MODE}.log"
  SESSION_TAG=""
fi

build_prompt() {
  local prompt
  prompt="$(cat "$PROMPT_FILE")"
  prompt="$(printf '%s\n' "$prompt" | sed "1s|^You are working in .*\\.$|You are working in ${WORKDIR}.|")"
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

  printf '%s\n' "$prompt"
}

render_command() {
  local prompt_text="$1"
  local cmd q_prompt q_workdir
  cmd="$CLAUDE_COMMAND"
  q_prompt="$(printf '%q' "$prompt_text")"
  q_workdir="$(printf '%q' "$WORKDIR")"

  if [[ "$cmd" == *"{workdir}"* ]]; then
    cmd="${cmd//\{workdir\}/$q_workdir}"
  fi
  if [[ "$cmd" == *"{prompt}"* ]]; then
    cmd="${cmd//\{prompt\}/$q_prompt}"
  else
    cmd="$cmd $q_prompt"
  fi

  printf '%s\n' "$cmd"
}

echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] starting mode=$MODE${SESSION_TAG} workdir=$WORKDIR log=$LOG_FILE shell=$SHELL_BIN command=$CLAUDE_COMMAND" | tee -a "$LOG_FILE"

iteration=0
while true; do
  iteration=$((iteration + 1))
  echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] mode=$MODE${SESSION_TAG} iteration=$iteration" | tee -a "$LOG_FILE"

  PROMPT_TEXT="$(build_prompt)"
  RENDERED_CMD="$(render_command "$PROMPT_TEXT")"
  RUN_TEXT="cd $(printf '%q' "$WORKDIR") && $RENDERED_CMD"
  CMD=( "$SHELL_BIN" -ic "$RUN_TEXT" )

  set +e
  if command -v timeout >/dev/null 2>&1; then
    timeout "$TIMEOUT_SECONDS" "${CMD[@]}" 2>&1 | tee -a "$LOG_FILE"
  else
    "${CMD[@]}" 2>&1 | tee -a "$LOG_FILE"
  fi
  status=${PIPESTATUS[0]:-0}
  set -e

  echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] iteration=$iteration exit_status=$status" | tee -a "$LOG_FILE"

  sleep "$SLEEP_SECONDS"
done
