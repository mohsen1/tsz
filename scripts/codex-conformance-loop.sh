#!/usr/bin/env bash
set -euo pipefail

CONFIG_FILE="${1:-scripts/codex-conformance-loop.yaml}"

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
APPROVAL_MODE="$(read_key approval_mode)"
BYPASS="$(read_key bypass_approvals_and_sandbox)"
SLEEP_SECS="$(read_key sleep_seconds)"
WORKDIR="$(read_key workdir)"
PROMPT_FILE="$(read_key prompt_file)"

MODEL="${MODEL:-gpt-5.3-codex}"
APPROVAL_MODE="${APPROVAL_MODE:-full-auto}"
BYPASS="${BYPASS:-false}"
SLEEP_SECS="${SLEEP_SECS:-2}"
WORKDIR="${WORKDIR:-$(pwd)}"
PROMPT_FILE="${PROMPT_FILE:-scripts/codex-conformance-loop.prompt.txt}"

if [[ ! -f "$PROMPT_FILE" ]]; then
  echo "Prompt file not found: $PROMPT_FILE" >&2
  exit 1
fi

mkdir -p logs
LOG_FILE="logs/codex-conformance-loop.log"

cd "$WORKDIR"

echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] starting loop" | tee -a "$LOG_FILE"

i=0
while true; do
  i=$((i + 1))
  echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] iteration=$i" | tee -a "$LOG_FILE"

  PROMPT_TEXT="$(tr '\n' ' ' < "$PROMPT_FILE")"

  set +e
  if [[ "$BYPASS" == "true" ]]; then
    codex "$PROMPT_TEXT" \
      --model "$MODEL" \
      --approval-mode "$APPROVAL_MODE" \
      --dangerously-bypass-approvals-and-sandbox \
      2>&1 | tee -a "$LOG_FILE"
  else
    codex "$PROMPT_TEXT" \
      --model "$MODEL" \
      --approval-mode "$APPROVAL_MODE" \
      2>&1 | tee -a "$LOG_FILE"
  fi
  status=$?
  set -e

  echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] iteration=$i exit_status=$status" | tee -a "$LOG_FILE"
  sleep "$SLEEP_SECS"
done
