#!/usr/bin/env bash
# run-session.sh — Multi-account CLI runner with auto-detection
#
# Reads a prompt from ./session.sh (executed, stdout captured) and runs it
# through Claude Code and/or Codex CLI, cycling through accounts when one
# is drained/rate-limited.
#
# Usage: ./scripts/run-session.sh [OPTIONS]

set -euo pipefail

# ─── Defaults ────────────────────────────────────────────────────────────────
DRY_RUN=false
RUN_ONCE=false
RUNNER_FILTER=""          # "" = all, "claude" or "codex"
SESSION_NAME=""           # "" = use existing session.sh, else load from scripts/sessions/
TIMEOUT_SECONDS=3600      # 1 hour
COOLDOWN_FALLBACK=1800    # 30 min fallback when reset time can't be parsed
LOOP_SLEEP=10             # seconds between loop iterations

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
SESSION_SCRIPT="$PROJECT_DIR/session.sh"

# Log directory: logs/sessions/YYYYMMDD/
LOG_BASE="$PROJECT_DIR/logs/sessions"

# ─── Runner state (parallel arrays for bash 3 compat) ───────────────────────
declare -a RUNNERS=()
declare -a DRAIN_KEYS=()    # runner specs that are drained
declare -a DRAIN_EPOCHS=()  # epoch when cooldown expires (parallel with DRAIN_KEYS)

# ─── Colors ────────────────────────────────────────────────────────────────
if [[ -t 2 ]]; then
  C_RESET=$'\033[0m'  C_DIM=$'\033[2m'  C_BOLD=$'\033[1m'
  C_RED=$'\033[31m'   C_GREEN=$'\033[32m' C_YELLOW=$'\033[33m'
  C_CYAN=$'\033[36m'  C_MAGENTA=$'\033[35m'
else
  C_RESET=""  C_DIM=""  C_BOLD=""
  C_RED=""    C_GREEN="" C_YELLOW=""
  C_CYAN=""   C_MAGENTA=""
fi

# ─── Logging ─────────────────────────────────────────────────────────────────
LOG_FILE=""   # set per-run

_ts() { date '+%Y-%m-%d %H:%M:%S'; }

# Terminal gets color, log file gets plain text
log() {
  local line="[$(_ts)] $*"
  echo "${C_DIM}${line}${C_RESET}" >&2
  [[ -n "$LOG_FILE" ]] && echo "$line" >> "$LOG_FILE" || true
}
warn() { echo "${C_YELLOW}[$(_ts)] WARN: $*${C_RESET}" >&2; [[ -n "$LOG_FILE" ]] && echo "[$(_ts)] WARN: $*" >> "$LOG_FILE" || true; }
err()  { echo "${C_RED}[$(_ts)] ERROR: $*${C_RESET}" >&2; [[ -n "$LOG_FILE" ]] && echo "[$(_ts)] ERROR: $*" >> "$LOG_FILE" || true; }
die()  { err "$*"; exit 1; }

# ─── Usage ───────────────────────────────────────────────────────────────────
usage() {
  cat <<'EOF'
Usage: run-session.sh [OPTIONS]

Runs ./session.sh prompt through available Claude/Codex accounts,
cycling to the next when one is drained or rate-limited.

Options:
  --session NAME     Use a session template from scripts/sessions/NAME.sh
  --dry-run          Show what would run without executing
  --once             Run once and exit (default: loop forever)
  --runner TYPE      Filter to "claude" or "codex" only
  --timeout N        Max seconds per runner execution (default: 3600)
  --cooldown N       Fallback cooldown seconds (default: 1800)
  --sleep N          Seconds between loop iterations (default: 10)
  -h, --help         Show this help

Available sessions:
  architect          Architecture audit + CI health check
  conformance-1      Conformance parity (first half, --max 6000)
  conformance-2      Conformance parity (second half, --offset 6000)
  emit               Emitter correctness
  lsp                LSP / fourslash tests
  perf               Performance benchmarking + optimization

Runners are auto-discovered from:
  ~/.claude            Default Claude Code account
  ~/.claude-*          Additional Claude Code accounts
  codex (if installed) Codex CLI
EOF
  exit 0
}

# ─── Argument parsing ────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    --session)    SESSION_NAME="${2:?'--session requires a value'}"; shift 2 ;;
    --dry-run)    DRY_RUN=true; shift ;;
    --once)       RUN_ONCE=true; shift ;;
    --runner)     RUNNER_FILTER="${2:?'--runner requires a value'}"; shift 2 ;;
    --timeout)    TIMEOUT_SECONDS="${2:?'--timeout requires a value'}"; shift 2 ;;
    --cooldown)   COOLDOWN_FALLBACK="${2:?'--cooldown requires a value'}"; shift 2 ;;
    --sleep)      LOOP_SLEEP="${2:?'--sleep requires a value'}"; shift 2 ;;
    -h|--help)    usage ;;
    *)            die "Unknown option: $1" ;;
  esac
done

# ─── Resolve timeout binary (once, at startup) ──────────────────────────────
TIMEOUT_BIN=""
if command -v timeout >/dev/null 2>&1; then
  TIMEOUT_BIN="timeout"
elif command -v gtimeout >/dev/null 2>&1; then
  TIMEOUT_BIN="gtimeout"
fi

# ─── Run with PTY + capture (cross-platform) ─────────────────────────────────
# Runs a command under `script` so the child gets a real PTY (output streams
# live, Ctrl+C propagates).  Captured output is written to $1.
# Usage: run_with_capture <outfile> <timeout_secs> <cmd...>
run_with_capture() {
  local outfile="$1" secs="$2"; shift 2

  # Build the command with timeout prefix
  local full_cmd=()
  if [[ -n "$TIMEOUT_BIN" ]]; then
    full_cmd+=("$TIMEOUT_BIN" "$secs")
  fi
  full_cmd+=("$@")

  if [[ "$(uname -s)" == "Darwin" ]]; then
    # macOS: script -q <file> <command> [args...]
    script -q "$outfile" "${full_cmd[@]}"
  else
    # Linux: script -qe -c "<command>" <file>
    local cmd_str
    cmd_str="$(printf '%q ' "${full_cmd[@]}")"
    script -qe -c "$cmd_str" "$outfile"
  fi
}

# ─── Cleanup trap ────────────────────────────────────────────────────────────
TMPFILES=()
cleanup() {
  local f
  for f in "${TMPFILES[@]+"${TMPFILES[@]}"}"; do
    [[ -f "$f" ]] && rm -f "$f"
  done
}
trap cleanup EXIT INT TERM

mktmp() {
  local f
  f="$(mktemp)"
  TMPFILES+=("$f")
  echo "$f"
}

# ─── Auto-discover runners ──────────────────────────────────────────────────
discover_runners() {
  RUNNERS=()

  # Default Claude account (~/.claude is the default config dir)
  if [[ -d "$HOME/.claude" ]] && command -v claude >/dev/null 2>&1; then
    RUNNERS+=("claude:$HOME/.claude")
  fi

  # Additional Claude accounts (~/.claude-*)
  for dir in "$HOME"/.claude-*; do
    [[ -d "$dir" && -f "$dir/.claude.json" ]] && RUNNERS+=("claude:$dir")
  done

  # Codex (if installed) — register spark and standard as separate runners
  # (they have independent credit pools)
  if command -v codex >/dev/null 2>&1; then
    RUNNERS+=("codex-spark:gpt-5.3-codex-spark")
    RUNNERS+=("codex:gpt-5.3-codex")
  fi

  # Apply filter ("codex" matches both codex and codex-spark)
  if [[ -n "$RUNNER_FILTER" ]]; then
    local filtered=()
    for r in "${RUNNERS[@]}"; do
      local rtype="${r%%:*}"
      case "$RUNNER_FILTER" in
        codex)  [[ "$rtype" == codex || "$rtype" == codex-spark ]] && filtered+=("$r") ;;
        *)      [[ "$rtype" == "$RUNNER_FILTER" ]] && filtered+=("$r") ;;
      esac
    done
    RUNNERS=("${filtered[@]}")
  fi
}

runner_type() { echo "${1%%:*}"; }
runner_path() { echo "${1#*:}"; }

runner_label() {
  local type path
  type="$(runner_type "$1")"
  path="$(runner_path "$1")"
  if [[ "$type" == "claude" ]]; then
    local dir_name
    dir_name="$(basename "$path")"
    echo "claude($dir_name)"
  elif [[ "$type" == "codex-spark" ]]; then
    echo "codex-spark($path)"
  elif [[ "$type" == "codex" ]]; then
    echo "codex($path)"
  else
    echo "$type"
  fi
}

# ─── Load prompt from session.sh ────────────────────────────────────────────
load_prompt() {
  if [[ ! -x "$SESSION_SCRIPT" ]]; then
    die "session.sh not found or not executable: $SESSION_SCRIPT"
  fi
  local prompt
  prompt="$("$SESSION_SCRIPT" 2>/dev/null)" || die "session.sh failed"
  if [[ -z "${prompt// /}" ]]; then
    die "session.sh produced empty prompt"
  fi
  echo "$prompt"
}

# ─── Parse Claude reset time ────────────────────────────────────────────────
# Extracts reset time from messages like:
#   "You're out of extra usage · resets 3pm (Europe/Berlin)"
# Returns seconds until reset, or fails (return 1).
parse_claude_reset_time() {
  local output="$1"

  # Use python3 for both parsing and timezone conversion (portable, no grep -P)
  local wait_seconds
  wait_seconds="$(python3 -c "
import re, datetime, zoneinfo, sys
text = sys.stdin.read()
m = re.search(r'resets\s+(\d{1,2}(?::\d{2})?)\s*(am|pm)\s*\(([^)]+)\)', text)
if not m: sys.exit(1)
time_str, ampm, tz_name = m.group(1), m.group(2), m.group(3)
try:
    tz = zoneinfo.ZoneInfo(tz_name)
except Exception:
    sys.exit(1)
now = datetime.datetime.now(tz)
parts = time_str.split(':')
h = int(parts[0])
mi = int(parts[1]) if len(parts) > 1 else 0
if ampm == 'pm' and h != 12: h += 12
if ampm == 'am' and h == 12: h = 0
reset = now.replace(hour=h, minute=mi, second=0, microsecond=0)
if reset <= now:
    reset += datetime.timedelta(days=1)
wait = int(reset.timestamp()) - int(now.timestamp())
print(max(wait, 0))
" <<< "$output" 2>/dev/null)" || return 1

  echo "$wait_seconds"
}

# ─── Drain management (parallel arrays for bash 3) ──────────────────────────

# Find index of key in DRAIN_KEYS, or return 1
_drain_index() {
  local key="$1" i
  for (( i=0; i<${#DRAIN_KEYS[@]}; i++ )); do
    [[ "${DRAIN_KEYS[$i]}" == "$key" ]] && echo "$i" && return 0
  done
  return 1
}

is_drained() {
  local key="$1" idx until now
  idx="$(_drain_index "$key")" || return 1
  until="${DRAIN_EPOCHS[$idx]}"
  now="$(date +%s)"
  (( now < until ))
}

mark_drained() {
  local key="$1" seconds="$2" now idx
  now="$(date +%s)"
  local expire=$(( now + seconds ))

  # Update existing or append
  if idx="$(_drain_index "$key")"; then
    DRAIN_EPOCHS[$idx]=$expire
  else
    DRAIN_KEYS+=("$key")
    DRAIN_EPOCHS+=("$expire")
  fi

  local until_str
  until_str="$(date -r "$expire" '+%H:%M:%S' 2>/dev/null || echo "~$((seconds/60))m")"
  echo "${C_MAGENTA}⏸ Marked $(runner_label "$key") as drained for ${seconds}s (until $until_str)${C_RESET}" >&2
  [[ -n "$LOG_FILE" ]] && echo "[$(_ts)] Marked $(runner_label "$key") as drained for ${seconds}s (until $until_str)" >> "$LOG_FILE" || true
}

# Returns seconds until the nearest cooldown expires, or 0 if none drained.
get_nearest_cooldown() {
  local now nearest=0 i
  now="$(date +%s)"
  for (( i=0; i<${#DRAIN_KEYS[@]}; i++ )); do
    local until="${DRAIN_EPOCHS[$i]}"
    if (( until > now )); then
      local remaining=$(( until - now ))
      if (( nearest == 0 || remaining < nearest )); then
        nearest=$remaining
      fi
    fi
  done
  echo "$nearest"
}

# ─── Drain detection patterns ───────────────────────────────────────────────
CLAUDE_DRAIN_PATTERNS=(
  "You're out of extra usage"
  "out of usage"
)
CLAUDE_RATE_PATTERNS=(
  "rate.limit"
  "rate_limit"
)
CODEX_DRAIN_PATTERNS=(
  "rate_limit_exceeded"
  "Rate limit reached"
  "stream disconnected"
  "429"
)

check_output_for_drain() {
  local type="$1" combined_output="$2"

  if [[ "$type" == "claude" ]]; then
    for pat in "${CLAUDE_DRAIN_PATTERNS[@]}"; do
      if echo "$combined_output" | grep -qi "$pat"; then
        return 0
      fi
    done
    for pat in "${CLAUDE_RATE_PATTERNS[@]}"; do
      if echo "$combined_output" | grep -qi "$pat"; then
        return 0
      fi
    done
  elif [[ "$type" == "codex" || "$type" == "codex-spark" ]]; then
    for pat in "${CODEX_DRAIN_PATTERNS[@]}"; do
      if echo "$combined_output" | grep -qi "$pat"; then
        return 0
      fi
    done
  fi
  return 1
}

# ─── Try a single runner ────────────────────────────────────────────────────
# Returns: 0=success, 1=drained, 2=error
try_runner() {
  local runner="$1"
  local prompt="$2"
  local type path label
  type="$(runner_type "$runner")"
  path="$(runner_path "$runner")"
  label="$(runner_label "$runner")"

  # Set up per-run log
  local day_dir
  day_dir="$LOG_BASE/$(date '+%Y%m%d')"
  mkdir -p "$day_dir"
  LOG_FILE="$day_dir/run-$(date '+%H%M%S')-${type}.log"

  echo "${C_GREEN}${C_BOLD}▶ Starting runner: $label${C_RESET}" >&2
  [[ -n "$LOG_FILE" ]] && echo "[$(_ts)] Starting runner: $label" >> "$LOG_FILE" || true
  log "Prompt (first 200 chars): ${prompt:0:200}..."

  local output_tmp
  output_tmp="$(mktmp)"

  local exit_code=0

  if [[ "$type" == "claude" ]]; then
    local cmd=(claude --dangerously-skip-permissions -p "$prompt")
    log "Command: CLAUDE_CONFIG_DIR=$path ${cmd[*]}"

    if $DRY_RUN; then
      log "[DRY-RUN] Would execute: CLAUDE_CONFIG_DIR=$path ${cmd[*]}"
      return 0
    fi

    set +e
    CLAUDE_CONFIG_DIR="$path" run_with_capture "$output_tmp" \
      "$TIMEOUT_SECONDS" "${cmd[@]}"
    exit_code=$?
    set -e

  elif [[ "$type" == "codex-spark" || "$type" == "codex" ]]; then
    # path holds the model name (e.g. gpt-5.3-codex-spark)
    local model="$path"
    local effort="medium"
    if [[ "$type" == "codex-spark" ]]; then
      effort="xhigh"
    fi
    local cmd=(codex exec -m "$model" -c "model_reasoning_effort=\"$effort\"" --dangerously-bypass-approvals-and-sandbox "$prompt")
    log "Command: ${cmd[*]}"

    if $DRY_RUN; then
      log "[DRY-RUN] Would execute: ${cmd[*]}"
      return 0
    fi

    set +e
    run_with_capture "$output_tmp" \
      "$TIMEOUT_SECONDS" "${cmd[@]}"
    exit_code=$?
    set -e

  else
    err "Unknown runner type: $type"
    return 2
  fi

  # Append captured output to log
  {
    echo "=== OUTPUT ==="
    cat "$output_tmp"
    echo "=== EXIT CODE: $exit_code ==="
  } >> "$LOG_FILE"

  # Check for drain/rate-limit
  local combined
  combined="$(cat "$output_tmp" 2>/dev/null || true)"

  if check_output_for_drain "$type" "$combined"; then
    warn "$label appears drained/rate-limited"

    # Try to parse smart cooldown from Claude output
    local cooldown_seconds="$COOLDOWN_FALLBACK"
    if [[ "$type" == "claude" ]]; then
      local parsed
      if parsed="$(parse_claude_reset_time "$combined")"; then
        cooldown_seconds="$parsed"
        log "Parsed reset time: ${cooldown_seconds}s until reset"
      else
        log "Could not parse reset time, using fallback: ${COOLDOWN_FALLBACK}s"
      fi
    fi

    mark_drained "$runner" "$cooldown_seconds"
    return 1
  fi

  # Timeout exit code (124 for timeout/gtimeout, 142 for perl SIGALRM)
  if [[ $exit_code -eq 124 || $exit_code -eq 142 ]]; then
    warn "$label timed out after ${TIMEOUT_SECONDS}s"
    return 2
  fi

  if [[ $exit_code -ne 0 ]]; then
    warn "$label exited with code $exit_code"
    return 2
  fi

  echo "${C_GREEN}✔ $label completed successfully${C_RESET}" >&2
  [[ -n "$LOG_FILE" ]] && echo "[$(_ts)] $label completed successfully" >> "$LOG_FILE" || true
  return 0
}

# ─── Run cycle: iterate runners ─────────────────────────────────────────────
run_cycle() {
  local prompt="$1"
  local tried=0 skipped=0

  for runner in "${RUNNERS[@]}"; do
    if is_drained "$runner"; then
      skipped=$((skipped + 1))
      log "Skipping $(runner_label "$runner") (drained, cooldown active)"
      continue
    fi

    tried=$((tried + 1))
    local result=0
    try_runner "$runner" "$prompt" || result=$?

    case $result in
      0) return 0 ;;   # Success
      1) continue ;;   # Drained, try next
      2) continue ;;   # Error, try next
    esac
  done

  # All runners tried or skipped
  if (( tried == 0 )); then
    # All runners are drained — wait for nearest cooldown
    local wait_seconds
    wait_seconds="$(get_nearest_cooldown)"
    if (( wait_seconds > 0 )); then
      log "All runners drained. Waiting ${wait_seconds}s for nearest cooldown..."
      sleep "$wait_seconds"
    else
      log "All runners drained but no cooldown set. Sleeping ${LOOP_SLEEP}s..."
      sleep "$LOOP_SLEEP"
    fi
    return 1
  fi

  warn "All tried runners failed or drained this cycle"
  return 1
}

# ─── Main ────────────────────────────────────────────────────────────────────
main() {
  # If --session NAME was given, copy template to session.sh
  if [[ -n "$SESSION_NAME" ]]; then
    local template="$SCRIPT_DIR/sessions/${SESSION_NAME}.sh"
    if [[ ! -f "$template" ]]; then
      die "Session template not found: $template"
    fi
    cp "$template" "$SESSION_SCRIPT"
    chmod +x "$SESSION_SCRIPT"
    log "Loaded session template: $SESSION_NAME"
  fi

  discover_runners

  if [[ ${#RUNNERS[@]} -eq 0 ]]; then
    die "No runners discovered. Ensure ~/.claude exists or codex is installed."
  fi

  log "Discovered ${#RUNNERS[@]} runner(s):"
  for r in "${RUNNERS[@]}"; do
    log "  - $(runner_label "$r") [$(runner_path "$r")]"
  done

  if $DRY_RUN; then
    log ""
    log "=== DRY RUN MODE ==="
    local prompt
    prompt="$(load_prompt)"
    log "Prompt from session.sh:"
    log "$prompt"
    log ""
    for runner in "${RUNNERS[@]}"; do
      try_runner "$runner" "$prompt"
    done
    exit 0
  fi

  local iteration=0
  while true; do
    iteration=$((iteration + 1))
    echo "" >&2
    echo "${C_CYAN}${C_BOLD}════════════════════════════════════════════════════════════════${C_RESET}" >&2
    echo "${C_CYAN}${C_BOLD}  Iteration #$iteration  $(date '+%Y-%m-%d %H:%M:%S')${C_RESET}" >&2
    echo "${C_CYAN}${C_BOLD}════════════════════════════════════════════════════════════════${C_RESET}" >&2

    # Reload prompt each iteration (picks up changes to session.sh)
    local prompt
    prompt="$(load_prompt)"

    run_cycle "$prompt" || true

    if $RUN_ONCE; then
      log "Single-shot mode (--once). Exiting."
      break
    fi

    log "Sleeping ${LOOP_SLEEP}s before next iteration..."
    sleep "$LOOP_SLEEP"
  done
}

main
