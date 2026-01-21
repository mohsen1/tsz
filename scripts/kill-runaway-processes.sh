#!/usr/bin/env bash
#
# Kill runaway processes from this Rust project
# Targets test/debug build artifacts that consume excessive CPU for too long
#
# Usage:
#   ./scripts/kill-runaway-processes.sh [OPTIONS]
#
# Options:
#   --daemon    : Start daemon mode (runs continuously in background)
#                 If daemon is already running, restarts it
#   --stop      : Stop the running daemon
#   --status    : Show daemon status
#   --dry-run   : Show what would be killed without actually killing
#   --check     : Run once and exit (default behavior)
#
# Examples:
#   # Run once (check and exit)
#   ./scripts/kill-runaway-processes.sh
#
#   # Start daemon in background
#   ./scripts/kill-runaway-processes.sh --daemon
#
#   # Restart daemon (stops existing, starts new)
#   ./scripts/kill-runaway-processes.sh --daemon
#
#   # Stop daemon
#   ./scripts/kill-runaway-processes.sh --stop
#
#   # Check daemon status
#   ./scripts/kill-runaway-processes.sh --status
#
#   # Dry run to see what would be killed
#   ./scripts/kill-runaway-processes.sh --dry-run
#

set -euo pipefail

# Configuration
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_NAME="tsz"
TARGET_DIR="$REPO_ROOT/target"
PID_FILE="$REPO_ROOT/.runaway-daemon.pid"
LOG_FILE="$REPO_ROOT/.runaway-daemon.log"

# Thresholds
MIN_CPU_PERCENT=150      # CPU % threshold (150% = 1.5 cores)
MIN_ELAPSED_MINUTES=20   # Minimum elapsed time before killing
DAEMON_CHECK_INTERVAL=300  # Check every 5 minutes (in seconds)

# State
DAEMON_MODE=false
STOP_DAEMON=false
SHOW_STATUS=false
DRY_RUN=false

# Parse arguments
ACTION="check"
for arg in "$@"; do
  case $arg in
    --daemon)
      DAEMON_MODE=true
      ACTION="daemon"
      ;;
    --daemon-internal)
      # Internal flag for daemon process, skip normal execution
      ACTION="daemon-internal"
      ;;
    --stop)
      STOP_DAEMON=true
      ACTION="stop"
      ;;
    --status)
      SHOW_STATUS=true
      ACTION="status"
      ;;
    --dry-run)
      DRY_RUN=true
      ;;
    --check)
      ACTION="check"
      ;;
    *)
      echo "Error: Unknown argument: $arg"
      echo ""
      echo "Usage: $0 [OPTIONS]"
      echo ""
      echo "Options:"
      echo "  --daemon    Start daemon mode (runs continuously in background)"
      echo "              If daemon is already running, restarts it"
      echo "  --stop      Stop the running daemon"
      echo "  --status    Show daemon status"
      echo "  --dry-run   Show what would be killed without actually killing"
      echo "  --check     Run once and exit (default)"
      exit 1
      ;;
  esac
done

# Logging function
log() {
  echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*"
}

# Get daemon PID from file
get_daemon_pid() {
  if [[ -f "$PID_FILE" ]]; then
    cat "$PID_FILE"
  fi
}

# Check if daemon is running
is_daemon_running() {
  local pid
  pid=$(get_daemon_pid)
  if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
    return 0
  fi
  return 1
}

# Stop the daemon
stop_daemon() {
  if ! is_daemon_running; then
    log "Daemon is not running"
    [[ -f "$PID_FILE" ]] && rm -f "$PID_FILE"
    return 0
  fi

  local pid
  pid=$(get_daemon_pid)
  log "Stopping daemon (PID $pid)..."
  kill "$pid" 2>/dev/null || true

  # Wait for process to end
  local count=0
  while kill -0 "$pid" 2>/dev/null && (( count < 10 )); do
    sleep 1
    ((count++))
  done

  # Force kill if still running
  if kill -0 "$pid" 2>/dev/null; then
    log "Force killing daemon..."
    kill -9 "$pid" 2>/dev/null || true
  fi

  rm -f "$PID_FILE"
  log "Daemon stopped"
}

# Show daemon status
show_status() {
  if is_daemon_running; then
    local pid
    pid=$(get_daemon_pid)
    log "Daemon is running (PID $pid)"
    log "Log file: $LOG_FILE"
    echo ""
    echo "Recent log entries:"
    tail -n 10 "$LOG_FILE" 2>/dev/null || echo "No log entries yet"
    return 0
  else
    log "Daemon is not running"
    [[ -f "$PID_FILE" ]] && rm -f "$PID_FILE"
    return 1
  fi
}

# Core function to check and kill runaway processes
check_runaway_processes() {
  local dry_run="$1"

  # Find and kill runaway processes
  runaway_procs=()

  while IFS= read -r line; do
    [[ -z "$line" ]] && continue

    pid=$(echo "$line" | awk '{print $1}')
    cpu_percent=$(echo "$line" | awk '{print $2}')
    elapsed_raw=$(echo "$line" | awk '{print $3}')
    comm=$(echo "$line" | awk '{print $4}')
    full_command=$(ps -p "$pid" -o command= 2>/dev/null || echo "")

    # Check if process is from this repo's target directory
    if [[ "$full_command" != *"$TARGET_DIR"* ]]; then
      continue
    fi

    # Parse elapsed time to minutes
    # Format can be: "12:34" (12h 34m), "1:23:45" (1h 23m 45s), or "45:12" (45m 12s)
    elapsed_minutes=0
    if [[ "$elapsed_raw" =~ ([0-9]+)-([0-9]+):([0-9]+):([0-9]+) ]]; then
      # DD-HH:MM:SS
      days="${BASH_REMATCH[1]}"
      hours="${BASH_REMATCH[2]}"
      minutes="${BASH_REMATCH[3]}"
      elapsed_minutes=$((days * 1440 + hours * 60 + minutes))
    elif [[ "$elapsed_raw" =~ ([0-9]+):([0-9]+):([0-9]+) ]]; then
      # HH:MM:SS
      hours="${BASH_REMATCH[1]}"
      minutes="${BASH_REMATCH[2]}"
      elapsed_minutes=$((hours * 60 + minutes))
    elif [[ "$elapsed_raw" =~ ([0-9]+):([0-9]+) ]]; then
      # MM:SS
      minutes="${BASH_REMATCH[1]}"
      elapsed_minutes=$minutes
    fi

    # Check thresholds
    if (( $(echo "$cpu_percent >= $MIN_CPU_PERCENT" | bc -l) )) && (( elapsed_minutes >= MIN_ELAPSED_MINUTES )); then
      runaway_procs+=("$pid|$cpu_percent|$elapsed_minutes|$full_command")
    fi
  done < <(ps aux | awk -v target_dir="$TARGET_DIR" 'index($11, target_dir) || index($12, target_dir) || index($13, target_dir) {
    print $2, $3, $10, $11
  }')

  # Kill the processes
  if [[ ${#runaway_procs[@]} -eq 0 ]]; then
    log "No runaway processes found from $REPO_NAME"
  else
    log "Found ${#runaway_procs[@]} runaway process(es):"
    for proc in "${runaway_procs[@]}"; do
      IFS='|' read -r pid cpu minutes full_command <<< "$proc"

      log "  PID $pid: ${cpu}% CPU, ${minutes}min elapsed"
      log "    Command: $full_command"

      if [[ "$dry_run" == true ]]; then
        log "    [DRY RUN] Would kill PID $pid"
      else
        log "    Killing PID $pid..."
        kill "$pid" 2>/dev/null || log "    Failed to kill PID $pid (may have already ended)"
      fi
    done
  fi
}

# Run as daemon
run_daemon() {
  # If daemon is already running, stop it first
  if is_daemon_running; then
    local pid
    pid=$(get_daemon_pid)
    log "Daemon already running (PID $pid), restarting..."
    stop_daemon
  fi

  # Start daemon in background
  log "Starting daemon in background..."
  log "Checking every $DAEMON_CHECK_INTERVAL seconds"
  log "Thresholds: CPU >= ${MIN_CPU_PERCENT}%, Time >= ${MIN_ELAPSED_MINUTES}min"

  # Write PID file
  echo $$ > "$PID_FILE"

  # Daemon main loop
  while true; do
    check_runaway_processes false
    sleep "$DAEMON_CHECK_INTERVAL"
  done
}

# Main execution
case "$ACTION" in
  daemon)
    # Detach from terminal and run as daemon
    nohup bash "$0" --daemon-internal > "$LOG_FILE" 2>&1 &
    sleep 2
    if is_daemon_running; then
      log "Daemon started successfully (PID $(get_daemon_pid))"
      log "Log file: $LOG_FILE"
    else
      log "Failed to start daemon"
      exit 1
    fi
    ;;
  daemon-internal)
    # Internal daemon entry point (used by nohup)
    run_daemon
    ;;
  stop)
    stop_daemon
    ;;
  status)
    show_status
    ;;
  check)
    check_runaway_processes "$DRY_RUN"
    ;;
esac

exit 0
