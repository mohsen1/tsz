#!/usr/bin/env bash
#
# Clean up build artifacts across all tsz worktree copies
#
# The tsz orchestrator creates multiple copies (tsz-1, tsz-2, ...) for parallel
# development. Each accumulates its own .target/ directory (Rust build artifacts)
# which can consume 10-60+ GB each. This script reclaims that disk space.
#
# Safety: Automatically detects active cargo/rustc builds and skips those
# directories to avoid corrupting in-progress compilations.
#
# Usage:
#   ./scripts/cleanup-build-artifacts.sh [OPTIONS]
#
# Options:
#   --all           Clean ALL tsz copies including the current one
#   --others        Clean only OTHER tsz copies (default)
#   --stale HOURS   Only clean .target dirs older than HOURS hours (default: 0)
#   --git-gc        Also run git gc on TypeScript submodules
#   --dry-run       Show what would be cleaned without actually deleting
#   --force         Skip active-build check (dangerous)
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
#   # Only clean targets not modified in 4+ hours
#   ./scripts/cleanup-build-artifacts.sh --stale 4
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
STALE_HOURS=0
GIT_GC=false
DRY_RUN=false
FORCE=false
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
      STALE_HOURS="$2"
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
    --force)
      FORCE=true
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
      head -45 "$0" | tail -40
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

# Capture all cargo/rustc process args once (expensive to call ps repeatedly).
# Called once at script start; functions below use this cached value.
_CARGO_PROCS=""
cache_active_procs() {
  _CARGO_PROCS=$(ps -eo args 2>/dev/null | grep -E '(cargo|rustc)' | grep -v grep || true)
}

# Get the set of active build profiles for a given repo's .target dir.
# Prints one profile name per line (e.g. "debug", "dist-fast").
get_active_profiles() {
  local target_dir="$1/.target"
  if [[ -z "$_CARGO_PROCS" ]]; then
    return
  fi
  echo "$_CARGO_PROCS" \
    | grep -oE "$target_dir/[a-zA-Z_-]+" \
    | sed "s|$target_dir/||" \
    | sort -u \
    || true
}

# Check if ANY build is active for a repo (used by --status)
is_build_active() {
  local profiles
  profiles=$(get_active_profiles "$1")
  [[ -n "$profiles" ]]
}

# Find all tsz copies — prints one directory per line for safe iteration
find_tsz_dirs() {
  for d in "$PARENT_DIR"/tsz-*/; do
    [[ -d "$d" ]] || continue
    [[ -f "$d/Cargo.toml" ]] || continue
    echo "${d%/}"
  done
}

# Show status
show_status() {
  echo "=== TSZ Worktree Disk Usage ==="
  echo ""
  printf "%-20s %10s %10s %10s %-8s %s\n" "DIRECTORY" "TOTAL" ".target" "TS/.git" "BUILD" "BRANCH"
  printf "%-20s %10s %10s %10s %-8s %s\n" "---------" "-----" "-------" "-------" "-----" "------"

  local total_target=0
  local total_overall=0
  local total_reclaimable=0

  while IFS= read -r d; do
    local name
    name="$(basename "$d")"
    local marker=""
    [[ "$d" == "$REPO_ROOT" ]] && marker=" *"

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

    local build_status="idle"
    if is_build_active "$d"; then
      build_status="ACTIVE"
    else
      total_reclaimable=$((total_reclaimable + target_size))
    fi

    local branch
    branch=$(git -C "$d" branch --show-current 2>/dev/null || echo "?")

    printf "%-20s %10s %10s %10s %-8s %s\n" \
      "${name}${marker}" \
      "$(human_size $((overall * 1024)))" \
      "$(human_size $((target_size * 1024)))" \
      "$(human_size $((ts_git_size * 1024)))" \
      "$build_status" \
      "$branch"
  done < <(find_tsz_dirs)

  echo ""
  printf "%-20s %10s %10s\n" "TOTAL" \
    "$(human_size $((total_overall * 1024)))" \
    "$(human_size $((total_target * 1024)))"
  echo ""
  echo "Reclaimable (idle .target dirs): $(human_size $((total_reclaimable * 1024)))"
  echo ""
  df -h / | head -2
}

# Clean build artifacts — profile-aware.
# For repos with active builds, deletes inactive profiles and incremental caches.
# For idle repos, deletes the entire .target directory.
clean_artifacts() {
  local total_freed=0
  local cleaned_profiles=0
  local cleaned_dirs=0
  local now
  now=$(date +%s)

  while IFS= read -r d; do
    # Skip current if mode is "others"
    if [[ "$MODE" == "others" && "$d" == "$REPO_ROOT" ]]; then
      continue
    fi

    local name
    name="$(basename "$d")"

    if [[ ! -d "$d/.target" ]]; then
      continue
    fi

    # Determine which profiles are actively building
    local active_profiles
    if [[ "$FORCE" == true ]]; then
      active_profiles=""
    else
      active_profiles=$(get_active_profiles "$d")
    fi

    if [[ -z "$active_profiles" ]]; then
      # No active builds — check staleness and delete entire .target
      local target_age_hours
      target_age_hours=$(( ( now - $(stat -f %m "$d/.target" 2>/dev/null || echo "0") ) / 3600 ))

      if (( target_age_hours >= STALE_HOURS )); then
        local size
        size=$(du -sk "$d/.target" 2>/dev/null | awk '{print $1}')
        size=${size:-0}

        if [[ "$DRY_RUN" == true ]]; then
          log "[DRY RUN] Would delete $name/.target ($(human_size $((size * 1024))), ${target_age_hours}h idle)"
        else
          log "Deleting $name/.target ($(human_size $((size * 1024))), ${target_age_hours}h idle)..."
          rm -rf "$d/.target" 2>/dev/null || true
          total_freed=$((total_freed + size))
          cleaned_dirs=$((cleaned_dirs + 1))
        fi
      fi
    else
      # Active build — clean per-profile
      log "$name: active profiles: $(echo "$active_profiles" | tr '\n' ' ')"

      for profile_dir in "$d/.target"/*/; do
        [[ -d "$profile_dir" ]] || continue
        local profile
        profile="$(basename "$profile_dir")"

        # Skip non-profile dirs (e.g. tmp/)
        [[ "$profile" == "tmp" ]] && continue

        if echo "$active_profiles" | grep -qxF "$profile"; then
          # Active profile — only clean incremental caches
          if [[ -d "$profile_dir/incremental" ]]; then
            local incr_size
            incr_size=$(du -sk "$profile_dir/incremental" 2>/dev/null | awk '{print $1}')
            incr_size=${incr_size:-0}

            if (( incr_size > 102400 )); then  # only bother if >100MB
              if [[ "$DRY_RUN" == true ]]; then
                log "[DRY RUN] Would clean $name/.target/$profile/incremental ($(human_size $((incr_size * 1024))))"
              else
                log "Cleaning $name/.target/$profile/incremental ($(human_size $((incr_size * 1024))))..."
                rm -rf "$profile_dir/incremental" 2>/dev/null || true
                total_freed=$((total_freed + incr_size))
                cleaned_profiles=$((cleaned_profiles + 1))
              fi
            fi
          fi
        else
          # Inactive profile — delete entirely
          local prof_size
          prof_size=$(du -sk "$profile_dir" 2>/dev/null | awk '{print $1}')
          prof_size=${prof_size:-0}

          if [[ "$DRY_RUN" == true ]]; then
            log "[DRY RUN] Would delete $name/.target/$profile ($(human_size $((prof_size * 1024))), inactive)"
          else
            log "Deleting $name/.target/$profile ($(human_size $((prof_size * 1024))), inactive)..."
            rm -rf "$profile_dir" 2>/dev/null || true
            total_freed=$((total_freed + prof_size))
            cleaned_profiles=$((cleaned_profiles + 1))
          fi
        fi
      done
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
  done < <(find_tsz_dirs)

  if [[ "$DRY_RUN" == true ]]; then
    log "Dry run complete. No files were deleted."
  else
    log "Freed $(human_size $((total_freed * 1024))) ($cleaned_dirs whole targets, $cleaned_profiles profiles/caches)"
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
        <string>4</string>
    </array>
    <key>StartInterval</key>
    <integer>21600</integer>
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
  log "  Schedule: Every 6 hours"
  log "  Cleans: All tsz-*/.target dirs older than 4 hours (skips active builds)"
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

# Rotate log file if it exceeds 1MB
rotate_log() {
  local log_file="$HOME/.tsz-cleanup.log"
  if [[ -f "$log_file" ]]; then
    local size
    size=$(stat -f %z "$log_file" 2>/dev/null || echo "0")
    if (( size > 1048576 )); then
      mv "$log_file" "${log_file}.old"
    fi
  fi
}

# Main
case "$ACTION" in
  status)
    cache_active_procs
    show_status
    ;;
  clean)
    rotate_log
    cache_active_procs
    clean_artifacts
    ;;
  install-daemon)
    install_daemon
    ;;
  uninstall-daemon)
    uninstall_daemon
    ;;
esac
