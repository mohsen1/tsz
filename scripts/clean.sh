#!/usr/bin/env bash
#
# Clean git-ignored artifacts from the current tsz repository.
#
# Removes build outputs, stray test files, caches, and other gitignored
# debris while preserving .env* files.
#
# Usage:
#   ./scripts/clean.sh [OPTIONS]
#
# Options:
#   --dry-run    Show what would be cleaned without deleting anything
#   --quiet      Suppress output (for use in git hooks)
#   -h, --help   Show this help
#
# Protected (never deleted):
#   .env, .env.local, .env.* — environment config files
#
# Examples:
#   ./scripts/clean.sh --dry-run    # Preview what would be removed
#   ./scripts/clean.sh              # Clean the repo
#   ./scripts/clean.sh --quiet      # Silent mode (git hooks)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

DRY_RUN=false
QUIET=false

# ── Argument parsing ────────────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
  case $1 in
    --dry-run) DRY_RUN=true; shift ;;
    --quiet)   QUIET=true; shift ;;
    -h|--help) sed -n '2,/^[^#]/{ /^#/s/^# \?//p; }' "$0"; exit 0 ;;
    *)         echo "Unknown option: $1 (try --help)"; exit 1 ;;
  esac
done

# ── Colors (disabled if not a tty or in quiet mode) ─────────────────────────

if [[ -t 1 ]] && [[ "$QUIET" == false ]]; then
  GREEN='\033[0;32m' CYAN='\033[0;36m' BOLD='\033[1m' DIM='\033[2m' RESET='\033[0m'
else
  GREEN='' CYAN='' BOLD='' DIM='' RESET=''
fi

log()     { [[ "$QUIET" == true ]] && return; echo -e "${GREEN}[clean]${RESET} $*"; }
dry_log() { echo -e "${CYAN}[dry-run]${RESET} $*"; }

# ── Helpers ─────────────────────────────────────────────────────────────────

human_size() {
  local kb=$1
  if (( kb >= 1048576 )); then
    printf "%.1fG" "$(echo "scale=1; $kb / 1048576" | bc)"
  elif (( kb >= 1024 )); then
    printf "%.0fM" "$(echo "scale=0; $kb / 1024" | bc)"
  else
    printf "%dK" "$kb"
  fi
}

dir_size_kb() {
  du -sk "$1" 2>/dev/null | awk '{print $1}' || echo 0
}

# ── Pathspec exclusions for .env files ──────────────────────────────────────
# git clean -e adds to ignore rules (wrong direction for -X mode).
# Pathspec negation ':!pattern' correctly excludes paths from the operation.

ENV_PATHSPEC=(':!.env' ':!.env.*' ':!.env.local')

# ── Dry run mode ────────────────────────────────────────────────────────────

if [[ "$DRY_RUN" == true ]]; then
  echo -e "${CYAN}${BOLD}DRY RUN — no files will be deleted${RESET}"
  echo ""

  # Show gitignored files that would be removed (excluding .env*)
  output=$(git -C "$REPO_ROOT" clean -nXd -- . "${ENV_PATHSPEC[@]}" 2>/dev/null || true)

  # Show untracked stray files in root only (test_*.js, tmp_*, etc.)
  # ':/' prefix anchors pathspecs to the repo root — avoids matching in subdirectories
  untracked=$(git -C "$REPO_ROOT" clean -nd -- \
    ':/*.js' ':/*.ts' ':/*.md' ':/*.py' ':/*.sh' 'tmp*' 'wasm/' "${ENV_PATHSPEC[@]}" 2>/dev/null || true)

  combined="${output}${output:+$'\n'}${untracked}"
  combined=$(echo "$combined" | sed '/^$/d' | sort -u)

  if [[ -z "$combined" ]]; then
    log "${DIM}Nothing to clean.${RESET}"
  else
    count=$(echo "$combined" | wc -l | tr -d ' ')
    log "Would remove $count items:"
    echo "$combined" | while IFS= read -r line; do
      dry_log "  $line"
    done
  fi
  exit 0
fi

# ── Actual cleanup ──────────────────────────────────────────────────────────

before_kb=$(dir_size_kb "$REPO_ROOT")

log "${BOLD}Cleaning $(basename "$REPO_ROOT")...${RESET}"

# Phase 1: Remove git-ignored files
# -X = ONLY ignored files (safe — won't touch untracked work-in-progress)
# -f = force, -d = include directories
# Pathspec ':!.env*' protects environment config files
git -C "$REPO_ROOT" clean -fXd -- . "${ENV_PATHSPEC[@]}" 2>/dev/null || true

# Phase 2: Remove untracked stray test/tmp files from repo root only
# The .gitignore has /*.js /*.ts etc. — ':/' anchors pathspecs to repo root
# so we don't accidentally delete legitimate files in subdirectories.
git -C "$REPO_ROOT" clean -fd -- \
  ':/*.js' ':/*.ts' ':/*.md' ':/*.py' ':/*.sh' 'tmp*' 'wasm/' "${ENV_PATHSPEC[@]}" 2>/dev/null || true

# Phase 3: Belt-and-suspenders for heavyweight dirs that git clean may skip
# (e.g. when they contain nested .git repos or permission issues)
for d in node_modules .target .target-bench target coverage logs artifacts pkg tmp; do
  [[ -d "$REPO_ROOT/$d" ]] && rm -rf "$REPO_ROOT/$d" 2>/dev/null || true
done

# Phase 4: Clean nested caches in scripts/ subdirectories
find "$REPO_ROOT/scripts" -maxdepth 3 -type d \
  \( -name "node_modules" -o -name "dist" -o -name ".cache" \) \
  -exec rm -rf {} + 2>/dev/null || true

# Phase 5: Remove .DS_Store files everywhere
find "$REPO_ROOT" -name ".DS_Store" -delete 2>/dev/null || true

# Phase 6: Remove stale package-lock.json files
find "$REPO_ROOT" -name "package-lock.json" -delete 2>/dev/null || true

# ── Report ──────────────────────────────────────────────────────────────────

after_kb=$(dir_size_kb "$REPO_ROOT")
freed_kb=$(( before_kb - after_kb ))

if (( freed_kb > 0 )); then
  log "Freed $(human_size $freed_kb) ($(human_size $before_kb) → $(human_size $after_kb))"
else
  log "${DIM}Already clean.${RESET}"
fi
