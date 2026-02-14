#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/codex-loop.sh [--conformance|--emit|--lsp|--arch|--spark] [--session N] [--config FILE] [--prompt-file FILE]

Modes:
  --conformance   Continuous conformance parity work (default)
  --emit          Continuous emitter-focused work
  --lsp           Continuous LSP-focused work
  --arch          Continuous architecture-roadmap execution loop
  --spark         Explicit spark mode (same prompt as conformance)

Options:
  --session N     Session id for parallel loops (also supports --session=N)
                  In conformance mode, session ids shard work by quarter using --offset/--max.
                  Also remaps workdir to sibling session dir (e.g. tsz-1 -> tsz-3).
  --config FILE   YAML config file (default: scripts/codex-loop.yaml)
  --prompt-file   Explicit prompt file override
  -h, --help      Show this help
EOF
}

CONFIG_FILE="scripts/codex-loop.yaml"
MODE=""
PROMPT_FILE_OVERRIDE=""
SESSION_ID=""

# Backward compatible: if first positional arg is a file, treat it as config path.
if [[ $# -gt 0 && "${1:-}" != -* && -f "$1" ]]; then
  CONFIG_FILE="$1"
  shift
fi

while [[ $# -gt 0 ]]; do
  case "$1" in
    --conformance)
      MODE="conformance"
      shift
      ;;
    --emit)
      MODE="emit"
      shift
      ;;
    --lsp)
      MODE="lsp"
      shift
      ;;
    --arch)
      MODE="arch"
      shift
      ;;
    --spark)
      MODE="spark"
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
    --config)
      CONFIG_FILE="${2:-}"
      shift 2
      ;;
    --prompt-file)
      PROMPT_FILE_OVERRIDE="${2:-}"
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

if [[ -n "$SESSION_ID" && ! "$SESSION_ID" =~ ^[0-9]+$ ]]; then
  echo "Invalid session id: $SESSION_ID (expected integer)" >&2
  exit 1
fi

if [[ ! -f "$CONFIG_FILE" ]]; then
  echo "Config file not found: $CONFIG_FILE" >&2
  exit 1
fi

if ! command -v codex >/dev/null 2>&1; then
  echo "codex CLI not found in PATH" >&2
  exit 1
fi

read_key() {
  local key="$1"
  sed -nE "s/^${key}:[[:space:]]*(.*)$/\1/p" "$CONFIG_FILE" | head -n1
}

MODEL="$(read_key model)"
MODEL_SPARK="$(read_key model_spark)"
CODEX_CLI_TIMEOUT="$(read_key command_timeout_seconds)"
# Backward-compat: accept approval_mode from previous YAML.
APPROVAL_MODE="$(read_key approval_mode)"
ASK_FOR_APPROVAL="$(read_key ask_for_approval)"
BYPASS="$(read_key bypass_approvals_and_sandbox)"
SLEEP_SECS="$(read_key sleep_seconds)"
WORKDIR="$(read_key workdir)"
DEFAULT_MODE="$(read_key default_mode)"
PROMPT_FILE_LEGACY="$(read_key prompt_file)"
PROMPT_FILE_CONF="$(read_key prompt_file_conformance)"
PROMPT_FILE_EMIT="$(read_key prompt_file_emit)"
PROMPT_FILE_LSP="$(read_key prompt_file_lsp)"
PROMPT_FILE_ARCH="$(read_key prompt_file_arch)"
ARCH_DOC="$(read_key architecture_doc)"
CONF_QUARTERS="$(read_key conformance_quarters)"
CONF_TOTAL_FAILURES="$(read_key conformance_total_failures)"

MODEL="${MODEL:-gpt-5.3-codex}"
MODEL_BASE="$MODEL"
CODEX_CLI_TIMEOUT="${CODEX_CLI_TIMEOUT:-120}"
APPROVAL_MODE="${APPROVAL_MODE:-full-auto}"
ASK_FOR_APPROVAL="${ASK_FOR_APPROVAL:-}"
BYPASS="${BYPASS:-false}"
SLEEP_SECS="${SLEEP_SECS:-2}"
WORKDIR="${WORKDIR:-.}"
DEFAULT_MODE="${DEFAULT_MODE:-conformance}"
CONF_QUARTERS="${CONF_QUARTERS:-4}"
CONF_TOTAL_FAILURES="${CONF_TOTAL_FAILURES:-3101}"
ARCH_DOC="${ARCH_DOC:-docs/architecture/CRITIQUE.md}"

if ! [[ "$CONF_QUARTERS" =~ ^[1-9][0-9]*$ ]]; then
  echo "Invalid conformance_quarters in config: $CONF_QUARTERS" >&2
  exit 1
fi
if ! [[ "$CONF_TOTAL_FAILURES" =~ ^[1-9][0-9]*$ ]]; then
  echo "Invalid conformance_total_failures in config: $CONF_TOTAL_FAILURES" >&2
  exit 1
fi

if [[ "$WORKDIR" == "~/"* ]]; then
  WORKDIR="${HOME}/${WORKDIR#~/}"
fi
if [[ "$WORKDIR" != /* ]]; then
  WORKDIR="$(pwd)/$WORKDIR"
fi
# Canonicalize existing paths (e.g. "/repo/." -> "/repo") before session remapping.
if [[ -d "$WORKDIR" ]]; then
  WORKDIR="$(cd "$WORKDIR" && pwd)"
fi

if [[ -n "$SESSION_ID" ]]; then
  if [[ "$WORKDIR" =~ ^(.+)-[0-9]+$ ]]; then
    WORKDIR="${BASH_REMATCH[1]}-${SESSION_ID}"
  else
    WORKDIR="${WORKDIR}-${SESSION_ID}"
  fi
fi

if [[ ! -d "$WORKDIR" ]]; then
  echo "Workdir not found: $WORKDIR" >&2
  exit 1
fi

if [[ -z "$MODE" ]]; then
  # Spark is opt-in: if configuration defaults to spark, keep non-spark behavior.
  if [[ "${DEFAULT_MODE:-}" == "spark" ]]; then
    MODE="conformance"
  else
    MODE="$DEFAULT_MODE"
  fi
fi

SPARK_MODEL_FALLBACK_ATTEMPTED="false"

if [[ "$MODE" == "spark" && -n "$MODEL_SPARK" ]]; then
  MODEL="$MODEL_SPARK"
fi

# Mode-specific prompt selection; fallback to legacy key, then hardcoded defaults.
if [[ -n "$PROMPT_FILE_OVERRIDE" ]]; then
  PROMPT_FILE="$PROMPT_FILE_OVERRIDE"
else
  case "$MODE" in
    conformance|spark)
      PROMPT_FILE="${PROMPT_FILE_CONF:-${PROMPT_FILE_LEGACY:-scripts/codex-loop.prompt.conformance.txt}}"
      ;;
    emit)
      PROMPT_FILE="${PROMPT_FILE_EMIT:-scripts/codex-loop.prompt.emit.txt}"
      ;;
    lsp)
      PROMPT_FILE="${PROMPT_FILE_LSP:-scripts/codex-loop.prompt.lsp.txt}"
      ;;
    arch)
      PROMPT_FILE="${PROMPT_FILE_ARCH:-scripts/codex-loop.prompt.arch.txt}"
      ;;
    *)
      echo "Unsupported mode: $MODE" >&2
      exit 1
      ;;
  esac
fi

if [[ ! -f "$PROMPT_FILE" ]]; then
  echo "Prompt file not found: $PROMPT_FILE" >&2
  exit 1
fi
if [[ "$MODE" == "arch" && ! -f "$ARCH_DOC" ]]; then
  echo "Architecture document not found: $ARCH_DOC" >&2
  exit 1
fi

mkdir -p logs
if [[ -n "$SESSION_ID" ]]; then
  LOG_FILE="logs/codex-loop.session-${SESSION_ID}.${MODE}.log"
  SESSION_TAG=" session=$SESSION_ID"
else
  LOG_FILE="logs/codex-loop.${MODE}.log"
  SESSION_TAG=""
fi

cd "$WORKDIR"

echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] starting loop mode=$MODE${SESSION_TAG} workdir=$WORKDIR config=$CONFIG_FILE prompt=$PROMPT_FILE" | tee -a "$LOG_FILE"

i=0
while true; do
  i=$((i + 1))
  echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] mode=$MODE${SESSION_TAG} iteration=$i" | tee -a "$LOG_FILE"

  PROMPT_TEXT="$(tr '\n' ' ' < "$PROMPT_FILE")"
  if [[ -n "$SESSION_ID" ]]; then
    PROMPT_TEXT="Session ${SESSION_ID}: ${PROMPT_TEXT}"
  fi
  if [[ "$MODE" == "conformance" && -n "$SESSION_ID" ]]; then
    # Shard conformance work across quarters so parallel sessions do not overlap.
    shard_index=$(( (SESSION_ID - 1) % CONF_QUARTERS ))
    shard_label=$(( shard_index + 1 ))
    shard_size=$(( (CONF_TOTAL_FAILURES + CONF_QUARTERS - 1) / CONF_QUARTERS ))
    shard_offset=$(( shard_index * shard_size ))
    remaining=$(( CONF_TOTAL_FAILURES - shard_offset ))
    if (( remaining < 0 )); then
      remaining=0
    fi
    shard_max=$shard_size
    if (( remaining < shard_max )); then
      shard_max=$remaining
    fi

    PROMPT_TEXT="${PROMPT_TEXT} Parallel conformance sharding: you own quarter ${shard_label}/${CONF_QUARTERS}. Focus only on failures in your shard. Use scripts/conformance.sh analyze --offset ${shard_offset} --max ${shard_max} for your slice, and keep fixes targeted to this slice."
  elif [[ "$MODE" == "arch" ]]; then
    PROMPT_TEXT="${PROMPT_TEXT} Architecture loop requirements for this iteration: (1) drive work from ${ARCH_DOC} roadmap items, (2) if you complete any roadmap item(s), update ${ARCH_DOC} in the same iteration and mark status done/progress explicitly, (3) run relevant tests for every code change and include exact test commands plus pass/fail results in your iteration summary, (4) do not mark roadmap work done without tests."
  fi

  CMD=(codex exec --model "$MODEL" -C "$WORKDIR" -c 'model_reasoning_effort="low"')

  if [[ "$BYPASS" == "true" ]]; then
    CMD+=(--dangerously-bypass-approvals-and-sandbox)
  else
    # Respect newer key first; fallback to legacy approval_mode mapping.
    if [[ -n "$ASK_FOR_APPROVAL" ]]; then
      CMD+=(-a "$ASK_FOR_APPROVAL")
    else
      case "$APPROVAL_MODE" in
        full-auto) CMD+=(--full-auto) ;;
        never|on-request|on-failure|untrusted) CMD+=(-a "$APPROVAL_MODE") ;;
        *) CMD+=(--full-auto) ;;
      esac
    fi
  fi

  TERMINAL_FAILURE_DETECTED="false"
  set +e
  ITER_PIPE="$(mktemp)"
  rm -f "$ITER_PIPE"
  mkfifo "$ITER_PIPE"
  if command -v timeout >/dev/null 2>&1; then
    timeout "$CODEX_CLI_TIMEOUT" "${CMD[@]}" "$PROMPT_TEXT" >"$ITER_PIPE" 2>&1 &
  else
    "${CMD[@]}" "$PROMPT_TEXT" >"$ITER_PIPE" 2>&1 &
  fi
  CODexLoopRun_PID=$!

  while IFS= read -r line <"$ITER_PIPE"; do
    echo "$line" | tee -a "$LOG_FILE"
    if [[ "$line" == *"You've hit your usage limit"* ]] \
      || [[ "$line" == *"state db missing rollout path"* ]] \
      || [[ "$line" == *"model_not_found"* ]] \
      || [[ "$line" == *"The requested model"* && "$line" == *"does not exist"* ]]; then
      TERMINAL_FAILURE_DETECTED="true"
      break
    fi
  done

  if [[ "$TERMINAL_FAILURE_DETECTED" == "true" ]]; then
    kill "$CODexLoopRun_PID" >/dev/null 2>&1 || true
  fi
  wait "$CODexLoopRun_PID" 2>/dev/null
  status=$?
  rm -f "$ITER_PIPE"

  set -e

  status="${status:-0}"
  echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] mode=$MODE${SESSION_TAG} iteration=$i exit_status=$status" | tee -a "$LOG_FILE"

  if [[ "$MODE" == "spark" && "$MODEL" != "$MODEL_BASE" && "$SPARK_MODEL_FALLBACK_ATTEMPTED" == "false" ]]; then
    if [[ "$TERMINAL_FAILURE_DETECTED" == "true" || "$status" -ne 0 ]]; then
      SPARK_MODEL_FALLBACK_ATTEMPTED="true"
      echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] mode=$MODE${SESSION_TAG} iteration=$i spark model unavailable (or terminal spark failure); falling back to model=$MODEL_BASE" | tee -a "$LOG_FILE"
      MODEL="$MODEL_BASE"
      continue
    fi
  fi

  if [[ "$TERMINAL_FAILURE_DETECTED" == "true" ]]; then
    echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] mode=$MODE${SESSION_TAG} iteration=$i hard-stop-trigger matched terminal failure" | tee -a "$LOG_FILE"
    exit 1
  fi

  if [[ "$MODE" == "spark" && "$status" -ne 0 ]]; then
    echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] mode=$MODE${SESSION_TAG} iteration=$i hard-stop: spark codex exit_status=$status" | tee -a "$LOG_FILE"
    exit "$status"
  fi

  sleep "$SLEEP_SECS"
done
