#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/codex-loop.sh [--conformance|--emit|--lsp] [--config FILE] [--prompt-file FILE]

Modes:
  --conformance   Continuous conformance parity work (default)
  --emit          Continuous emitter-focused work
  --lsp           Continuous LSP-focused work

Options:
  --config FILE   YAML config file (default: scripts/codex-loop.yaml)
  --prompt-file   Explicit prompt file override
  -h, --help      Show this help
EOF
}

CONFIG_FILE="scripts/codex-loop.yaml"
MODE=""
PROMPT_FILE_OVERRIDE=""

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

MODEL="${MODEL:-gpt-5.3-codex}"
APPROVAL_MODE="${APPROVAL_MODE:-full-auto}"
ASK_FOR_APPROVAL="${ASK_FOR_APPROVAL:-}"
BYPASS="${BYPASS:-false}"
SLEEP_SECS="${SLEEP_SECS:-2}"
WORKDIR="${WORKDIR:-$(pwd)}"
DEFAULT_MODE="${DEFAULT_MODE:-conformance}"

if [[ -z "$MODE" ]]; then
  # Spark is opt-in: if configuration defaults to spark, keep non-spark behavior.
  if [[ "${DEFAULT_MODE:-}" == "spark" ]]; then
    MODE="conformance"
  else
    MODE="$DEFAULT_MODE"
  fi
fi

# Mode-specific prompt selection; fallback to legacy key, then hardcoded defaults.
if [[ -n "$PROMPT_FILE_OVERRIDE" ]]; then
  PROMPT_FILE="$PROMPT_FILE_OVERRIDE"
else
  case "$MODE" in
    conformance)
      PROMPT_FILE="${PROMPT_FILE_CONF:-${PROMPT_FILE_LEGACY:-scripts/codex-loop.prompt.conformance.txt}}"
      ;;
    emit)
      PROMPT_FILE="${PROMPT_FILE_EMIT:-scripts/codex-loop.prompt.emit.txt}"
      ;;
    lsp)
      PROMPT_FILE="${PROMPT_FILE_LSP:-scripts/codex-loop.prompt.lsp.txt}"
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

mkdir -p logs
LOG_FILE="logs/codex-loop.log"

cd "$WORKDIR"

echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] starting loop mode=$MODE config=$CONFIG_FILE prompt=$PROMPT_FILE" | tee -a "$LOG_FILE"

i=0
while true; do
  i=$((i + 1))
  echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] mode=$MODE iteration=$i" | tee -a "$LOG_FILE"

  PROMPT_TEXT="$(tr '\n' ' ' < "$PROMPT_FILE")"

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

  set +e
  "${CMD[@]}" "$PROMPT_TEXT" 2>&1 | tee -a "$LOG_FILE"
  status=$?
  set -e

  echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] mode=$MODE iteration=$i exit_status=$status" | tee -a "$LOG_FILE"
  sleep "$SLEEP_SECS"
done
