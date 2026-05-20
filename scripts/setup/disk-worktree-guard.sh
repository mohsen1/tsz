#!/usr/bin/env bash
#
# Compact disk/worktree guard for agents.
#
# This intentionally avoids broad `du` reports. Use it before creating a new
# worktree or before starting a large build when disk pressure is suspected.
#
# Usage:
#   scripts/setup/disk-worktree-guard.sh
#   scripts/setup/disk-worktree-guard.sh --auto-prune
#
# Environment:
#   TSZ_DISK_MIN_FREE_GB       minimum free space before warning/pruning (default: 20)
#   TSZ_WORKTREE_INACTIVE_HOURS minimum age for reuse candidates (default: 4)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
REPO_PARENT="$(dirname "$REPO_ROOT")"

MIN_FREE_GB="${TSZ_DISK_MIN_FREE_GB:-20}"
INACTIVE_HOURS="${TSZ_WORKTREE_INACTIVE_HOURS:-4}"
AUTO_PRUNE=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --auto-prune) AUTO_PRUNE=true; shift ;;
    -h|--help) sed -n '2,/^$/p' "$0" | sed 's/^# \?//'; exit 0 ;;
    *) echo "Unknown option: $1" >&2; exit 2 ;;
  esac
done

df_kb="$(df -Pk "$REPO_PARENT" | awk 'NR==2 {print $4}')"
free_gb=$(( df_kb / 1024 / 1024 ))

printf 'disk_free_gb=%s path=%s\n' "$free_gb" "$REPO_PARENT"

prune_incremental() {
  local pruned=0
  while IFS= read -r wt; do
    [[ "$wt" == "$REPO_PARENT"/* ]] || continue
    for tdir in "$wt/target" "$wt/.target" "$wt/.target-bench"; do
      [[ -d "$tdir" ]] || continue
      while IFS= read -r inc; do
        find "$inc" -mindepth 1 -maxdepth 1 -type d -mtime +7 \
          -exec rm -rf {} + 2>/dev/null || true
        pruned=1
      done < <(find "$tdir" -type d -name incremental -mindepth 2 -maxdepth 4 2>/dev/null)
    done
  done < <(git -C "$REPO_ROOT" worktree list --porcelain | awk '/^worktree / {print substr($0, 10)}')
  [[ "$pruned" -eq 1 ]] && echo "pruned=old-cargo-incremental" || echo "pruned=none"
}

if (( free_gb < MIN_FREE_GB )); then
  printf 'disk_status=low min_free_gb=%s\n' "$MIN_FREE_GB"
  if [[ "$AUTO_PRUNE" == true ]]; then
    prune_incremental
    df_kb="$(df -Pk "$REPO_PARENT" | awk 'NR==2 {print $4}')"
    free_gb=$(( df_kb / 1024 / 1024 ))
    printf 'disk_free_gb_after=%s\n' "$free_gb"
  fi
else
  printf 'disk_status=ok min_free_gb=%s\n' "$MIN_FREE_GB"
fi

cutoff_minutes=$(( INACTIVE_HOURS * 60 ))
echo "sister_worktree_reuse_candidates:"

git -C "$REPO_ROOT" worktree list --porcelain \
  | awk '
      /^worktree / { if (path) print path "\t" branch; path=substr($0,10); branch="" }
      /^branch / { branch=substr($0,8) }
      END { if (path) print path "\t" branch }
    ' \
  | while IFS=$'\t' read -r wt branch; do
      [[ "$wt" == "$REPO_PARENT"/* ]] || continue
      [[ "$wt" != "$REPO_ROOT" ]] || continue
      [[ -d "$wt" ]] || continue

      recent="$(
        find "$wt" \
          \( -path "$wt/.git" -o -path "$wt/target" -o -path "$wt/.target" \
             -o -path "$wt/.target-bench" -o -path "$wt/node_modules" \
             -o -path "$wt/TypeScript" \) -prune \
          -o -type f -mmin "-$cutoff_minutes" -print -quit 2>/dev/null
      )"

      if [[ -z "$recent" ]]; then
        printf '  %s branch=%s inactive_hours>=%s\n' "$wt" "${branch:-unknown}" "$INACTIVE_HOURS"
      fi
    done
