#!/usr/bin/env bash
#
# Clean git-ignored artifacts from the current tsz repository.
#
# By default, preserves Rust build caches (.target/, .target-bench/) so
# incremental compilation still works. Use --full to nuke everything.
#
# Usage:
#   ./scripts/clean.sh [OPTIONS]
#
# Options:
#   --dry-run    Show what would be cleaned without deleting anything
#   --full       Also remove Rust build caches (.target/, .target-bench/)
#   --quiet      Suppress output (for use in git hooks)
#   -h, --help   Show this help
#
# Protected (never deleted without --full):
#   .target/, .target-bench/ — Rust incremental build caches
#
# Protected (never deleted):
#   .env, .env.local, .env.* — environment config files
#
# Examples:
#   ./scripts/clean.sh --dry-run    # Preview what would be removed
#   ./scripts/clean.sh              # Clean debris, keep build caches
#   ./scripts/clean.sh --full       # Nuke everything including build caches
#   ./scripts/clean.sh --quiet      # Silent mode (git hooks)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

DRY_RUN=false
QUIET=false
FULL=false

# ── Argument parsing ────────────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
  case $1 in
    --dry-run) DRY_RUN=true; shift ;;
    --full)    FULL=true; shift ;;
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

# ── Protected patterns ─────────────────────────────────────────────────────
# git clean pathspec negation (:!pattern) does NOT reliably exclude directories
# in -X mode, so we filter the candidate list ourselves instead.

# Always protected
PROTECTED_RE='^(\.env|\.env\.)'

# Unless --full, also protect Rust build caches
if [[ "$FULL" == false ]]; then
  PROTECTED_RE='^(\.env|\.env\.|\.target\/|\.target-bench\/|target\/)'
fi

# Filter helper: reads "Would remove X" lines from git clean -n, strips protected
filter_candidates() {
  while IFS= read -r line; do
    # git clean -n outputs "Would remove <path>"
    local path="${line#Would remove }"
    if [[ "$path" =~ $PROTECTED_RE ]]; then
      continue
    fi
    echo "$line"
  done
}

# ── Dry run mode ────────────────────────────────────────────────────────────

if [[ "$DRY_RUN" == true ]]; then
  echo -e "${CYAN}${BOLD}DRY RUN — no files will be deleted${RESET}"
  echo ""

  # Show gitignored files that would be removed
  output=$(git -C "$REPO_ROOT" clean -nXd -- . 2>/dev/null | filter_candidates || true)

  # Show untracked stray files in root only (test_*.js, tmp_*, etc.)
  # ':/' prefix anchors pathspecs to the repo root — avoids matching in subdirectories
  untracked=$(git -C "$REPO_ROOT" clean -nd -- \
    ':/*.js' ':/*.ts' ':/*.md' ':/*.py' ':/*.sh' 'tmp*' 'wasm/' 2>/dev/null | filter_candidates || true)

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

# Phase 1: Remove git-ignored files (filtered to preserve build caches)
# List candidates with -n, filter out protected paths, then remove each.
# This avoids git clean's lack of reliable directory exclusion in -X mode.
git -C "$REPO_ROOT" clean -nXd -- . 2>/dev/null \
  | filter_candidates \
  | while IFS= read -r line; do
      path="${line#Would remove }"
      rm -rf "$REPO_ROOT/$path" 2>/dev/null || true
    done

# Phase 2: Remove untracked stray test/tmp files from repo root only
# The .gitignore has /*.js /*.ts etc. — ':/' anchors pathspecs to repo root
# so we don't accidentally delete legitimate files in subdirectories.
git -C "$REPO_ROOT" clean -fd -- \
  ':/*.js' ':/*.ts' ':/*.md' ':/*.py' ':/*.sh' 'tmp*' 2>/dev/null || true

# Phase 3: Belt-and-suspenders for heavyweight dirs that git clean may skip
# (e.g. when they contain nested .git repos or permission issues)
CLEAN_DIRS=(node_modules coverage artifacts pkg tmp)
if [[ "$FULL" == true ]]; then
  CLEAN_DIRS+=(.target .target-bench target)
fi
for d in "${CLEAN_DIRS[@]}"; do
  [[ -d "$REPO_ROOT/$d" ]] && rm -rf "$REPO_ROOT/$d" 2>/dev/null || true
done

# Phase 4: Clean nested caches in scripts/ subdirectories
find "$REPO_ROOT/scripts" -maxdepth 3 -type d \
  \( -name "node_modules" -o -name "dist" -o -name ".cache" \) \
  -exec rm -rf {} + 2>/dev/null || true

# Phase 5: Remove .DS_Store files everywhere
find "$REPO_ROOT" -name ".DS_Store" -delete 2>/dev/null || true

# Phase 6: Remove stale package-lock.json files
find "$REPO_ROOT" -name "package-lock.json" -not -path "*/TypeScript/*" -delete 2>/dev/null || true

# Phase 7: Remove bench/profiling leftovers
# - typescript/ (lowercase) is a tsc build clone from bench scripts (not the TypeScript/ submodule)
# - perf.data*, flamegraph.svg, *.actual from conformance/profiling
if [[ -d "$REPO_ROOT/typescript" ]]; then
  # May contain read-only files from node_modules/.git — force writable first
  chmod -R u+w "$REPO_ROOT/typescript" 2>/dev/null || true
  rm -rf "$REPO_ROOT/typescript"
fi
rm -f "$REPO_ROOT"/perf.data* "$REPO_ROOT"/*.svg "$REPO_ROOT"/flamegraph* 2>/dev/null || true
rm -f "$REPO_ROOT"/*.actual "$REPO_ROOT"/*v8.log 2>/dev/null || true
rm -f "$REPO_ROOT"/.conformance-value 2>/dev/null || true

# Phase 8: Remove stale session logs older than 7 days
find "$REPO_ROOT/logs" -type f -mtime +7 -delete 2>/dev/null || true
find "$REPO_ROOT/logs" -type d -empty -delete 2>/dev/null || true

# Phase 9: Remove script(1) typescript files left by run-session.sh PTY capture
rm -f "$REPO_ROOT"/typescript.* 2>/dev/null || true

# Phase 10: Clean conformance temp caches
rm -rf "$REPO_ROOT/scripts/conformance/.tsc-cache" 2>/dev/null || true
rm -rf "$REPO_ROOT/scripts/conformance/dist" 2>/dev/null || true
rm -f "$REPO_ROOT"/tsc-cache*.json 2>/dev/null || true
# Keep the committed tsc-cache-full.json
git -C "$REPO_ROOT" checkout -- scripts/tsc-cache-full.json 2>/dev/null || true

# Phase 11: Reset TypeScript submodule to clean state
git -C "$REPO_ROOT" submodule update --force 2>/dev/null || true

# ── Report ──────────────────────────────────────────────────────────────────

after_kb=$(dir_size_kb "$REPO_ROOT")
freed_kb=$(( before_kb - after_kb ))

if (( freed_kb > 0 )); then
  log "Freed $(human_size $freed_kb) ($(human_size $before_kb) → $(human_size $after_kb))"
else
  log "${DIM}Already clean.${RESET}"
fi
