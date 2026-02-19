#!/usr/bin/env bash
#
# Clean up build artifacts across all tsz worktree copies
#
# The tsz orchestrator creates multiple copies (tsz-1, tsz-2, ...) for parallel
# development. Each accumulates its own .target/ directory (Rust build artifacts)
# which can consume 1-11 GB each. This script reclaims that disk space.
#
# Usage:
#   ./scripts/cleanup-build-artifacts.sh [OPTIONS]
#
# Options:
#   --all           Clean ALL tsz copies including the current one
#   --others        Clean only OTHER tsz copies (default)
#   --stale DAYS    Only clean .target dirs older than DAYS days (default: 1)
#   --git-gc        Also run git gc on TypeScript submodules
#   --dry-run       Show what would be cleaned without actually deleting
#   --daemon        Install a launchd agent for periodic automatic cleanup
#   --uninstall     Remove the launchd agent
#   --status        Show disk usage across all tsz copies
#
# Examples:
#   # See disk usage across all copies
#   ./scripts/cleanup-build-artifacts.sh --status
#
#   # Clean all other tsz copies' build artifacts
#   ./scripts/cleanup-build-artifacts.sh
#
#   # Clean everything including current worktree
#   ./scripts/cleanup-build-artifacts.sh --all
#
#   # Only clean targets not modified in 3+ days
#   ./scripts/cleanup-build-artifacts.sh --stale 3
#
#   # Install automatic daily cleanup
#   ./scripts/cleanup-build-artifacts.sh --daemon
#

set -euo pipefail

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PARENT_DIR="$(dirname "$REPO_ROOT")"
CURRENT_DIR_NAME="$(basename "$REPO_ROOT")"
LAUNCHD_LABEL="com.tsz.cleanup-build-artifacts"
LAUNCHD_PLIST="$HOME/Library/LaunchAgents/${LAUNCHD_LABEL}.plist"

# Defaults
MODE="others"
STALE_DAYS=0
GIT_GC=false
DRY_RUN=false
ACTION="clean"

# Parse arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --all)
      MODE="all"
      shift
      ;;
    --others)
      MODE="others"
      shift
      ;;
    --stale)
      STALE_DAYS="$2"
      shift 2
      ;;
    --git-gc)
      GIT_GC=true
      shift
      ;;
    --dry-run)
      DRY_RUN=true
      shift
      ;;
    --daemon)
      ACTION="install-daemon"
      shift
      ;;
    --uninstall)
      ACTION="uninstall-daemon"
      shift
      ;;
    --status)
      ACTION="status"
      shift
      ;;
    -h|--help)
      head -40 "$0" | tail -35
      exit 0
      ;;
    *)
      echo "Error: Unknown argument: $1"
      echo "Run with --help for usage"
      exit 1
      ;;
  esac
done

log() {
  echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*"
}

human_size() {
  local bytes=$1
  if (( bytes >= 1073741824 )); then
    echo "$(echo "scale=1; $bytes / 1073741824" | bc)G"
  elif (( bytes >= 1048576 )); then
    echo "$(echo "scale=0; $bytes / 1048576" | bc)M"
  else
    echo "${bytes}B"
  fi
}

# Find all tsz copies
find_tsz_dirs() {
  local dirs=()
  for d in "$PARENT_DIR"/tsz-*/; do
    [[ -d "$d" ]] || continue
    # Must look like a tsz checkout (has Cargo.toml)
    [[ -f "$d/Cargo.toml" ]] || continue
    dirs+=("${d%/}")
  done
  echo "${dirs[@]}"
}

# Show status
show_status() {
  echo "=== TSZ Worktree Disk Usage ==="
  echo ""
  printf "%-25s %10s %10s %10s %s\n" "DIRECTORY" "TOTAL" ".target" "TS/.git" "BRANCH"
  printf "%-25s %10s %10s %10s %s\n" "---------" "-----" "-------" "-------" "------"

  local total_target=0
  local total_overall=0

  for d in $(find_tsz_dirs); do
    local name
    name="$(basename "$d")"
    local marker=""
    [[ "$d" == "$REPO_ROOT" ]] && marker=" ←"

    local overall
    overall=$(du -sk "$d" 2>/dev/null | awk '{print $1}')
    overall=${overall:-0}
    total_overall=$((total_overall + overall))

    local target_size=0
    if [[ -d "$d/.target" ]]; then
      target_size=$(du -sk "$d/.target" 2>/dev/null | awk '{print $1}')
      target_size=${target_size:-0}
      total_target=$((total_target + target_size))
    fi

    local ts_git_size=0
    if [[ -d "$d/TypeScript/.git" ]]; then
      ts_git_size=$(du -sk "$d/TypeScript/.git" 2>/dev/null | awk '{print $1}')
      ts_git_size=${ts_git_size:-0}
    fi

    local branch
    branch=$(git -C "$d" branch --show-current 2>/dev/null || echo "?")

    printf "%-25s %10s %10s %10s %s\n" \
      "${name}${marker}" \
      "$(human_size $((overall * 1024)))" \
      "$(human_size $((target_size * 1024)))" \
      "$(human_size $((ts_git_size * 1024)))" \
      "$branch"
  done

  echo ""
  printf "%-25s %10s %10s\n" "TOTAL" \
    "$(human_size $((total_overall * 1024)))" \
    "$(human_size $((total_target * 1024)))"
  echo ""
  echo "Reclaimable (.target dirs): $(human_size $((total_target * 1024)))"
  echo ""
  df -h / | head -2
}

# Clean build artifacts
clean_artifacts() {
  local cleaned=0
  local total_freed=0

  for d in $(find_tsz_dirs); do
    # Skip current if mode is "others"
    if [[ "$MODE" == "others" && "$d" == "$REPO_ROOT" ]]; then
      continue
    fi

    local name
    name="$(basename "$d")"

    # Clean .target directory
    if [[ -d "$d/.target" ]]; then
      # Check staleness
      local target_age_days
      target_age_days=$(( ( $(date +%s) - $(stat -f %m "$d/.target" 2>/dev/null || echo "0") ) / 86400 ))

      if (( target_age_days >= STALE_DAYS )); then
        local size
        size=$(du -sk "$d/.target" 2>/dev/null | awk '{print $1}')
        size=${size:-0}

        if [[ "$DRY_RUN" == true ]]; then
          log "[DRY RUN] Would clean $name/.target ($(human_size $((size * 1024))), ${target_age_days}d old)"
        else
          log "Cleaning $name/.target ($(human_size $((size * 1024))), ${target_age_days}d old)..."
          rm -rf "$d/.target" 2>/dev/null || {
            # Some files may be locked by running processes; clean what we can
            find "$d/.target" -type f -delete 2>/dev/null || true
            find "$d/.target" -type d -empty -delete 2>/dev/null || true
            log "  Warning: some files in $name/.target could not be removed (may be in use)"
          }
          total_freed=$((total_freed + size))
          ((cleaned++))
        fi
      fi
    fi

    # Git GC on TypeScript submodule
    if [[ "$GIT_GC" == true && -d "$d/TypeScript/.git" ]]; then
      if [[ "$DRY_RUN" == true ]]; then
        log "[DRY RUN] Would run git gc in $name/TypeScript"
      else
        log "Running git gc in $name/TypeScript..."
        git -C "$d/TypeScript" gc --auto --quiet 2>/dev/null || true
      fi
    fi
  done

  if [[ "$DRY_RUN" == true ]]; then
    log "Dry run complete. No files were deleted."
  else
    log "Cleaned $cleaned target directories, freed $(human_size $((total_freed * 1024)))"
  fi
}

# Install launchd agent for periodic cleanup
install_daemon() {
  local script_path="$SCRIPT_DIR/cleanup-build-artifacts.sh"

  mkdir -p "$HOME/Library/LaunchAgents"

  cat > "$LAUNCHD_PLIST" << PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>${LAUNCHD_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>/bin/bash</string>
        <string>${script_path}</string>
        <string>--all</string>
        <string>--stale</string>
        <string>1</string>
    </array>
    <key>StartCalendarInterval</key>
    <dict>
        <key>Hour</key>
        <integer>4</integer>
        <key>Minute</key>
        <integer>0</integer>
    </dict>
    <key>StandardOutPath</key>
    <string>${HOME}/.tsz-cleanup.log</string>
    <key>StandardErrorPath</key>
    <string>${HOME}/.tsz-cleanup.log</string>
    <key>RunAtLoad</key>
    <false/>
</dict>
</plist>
PLIST

  # Unload if already loaded, then load
  launchctl unload "$LAUNCHD_PLIST" 2>/dev/null || true
  launchctl load "$LAUNCHD_PLIST"

  log "Installed launchd agent: $LAUNCHD_LABEL"
  log "  Schedule: Daily at 4:00 AM"
  log "  Cleans: All tsz-*/.target dirs older than 1 day"
  log "  Plist: $LAUNCHD_PLIST"
  log "  Log: ~/.tsz-cleanup.log"
  log ""
  log "To uninstall: $0 --uninstall"
}

# Uninstall launchd agent
uninstall_daemon() {
  if [[ -f "$LAUNCHD_PLIST" ]]; then
    launchctl unload "$LAUNCHD_PLIST" 2>/dev/null || true
    rm -f "$LAUNCHD_PLIST"
    log "Removed launchd agent: $LAUNCHD_LABEL"
  else
    log "No launchd agent found at $LAUNCHD_PLIST"
  fi
}

# Main
case "$ACTION" in
  status)
    show_status
    ;;
  clean)
    clean_artifacts
    ;;
  install-daemon)
    install_daemon
    ;;
  uninstall-daemon)
    uninstall_daemon
    ;;
esac
