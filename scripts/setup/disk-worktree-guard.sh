#!/usr/bin/env bash
#
# Compact disk/worktree guard for agents.
#
# This intentionally avoids broad `du` reports. Use it before creating a new
# worktree or before starting a large build when disk pressure is suspected.
#
# Usage:
#   scripts/setup/disk-worktree-guard.sh
#   scripts/setup/disk-worktree-guard.sh --json-report /tmp/tsz-disk-guard.json
#   scripts/setup/disk-worktree-guard.sh --auto-prune
#
# Environment:
#   TSZ_DISK_MIN_FREE_GB       minimum free space before warning/pruning (default: 20)
#   TSZ_WORKTREE_INACTIVE_HOURS minimum age for reuse candidates (default: 4)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"
REPO_PARENT="$(dirname "$REPO_ROOT")"
WORKTREE_PARENT="$REPO_PARENT"

# Codex app worktrees are nested one level deeper, for example:
#   .../.codex/worktrees/e61d/tsz
# Reuse candidates live beside the hash directory, not inside it.
if [[ "$(basename "$REPO_ROOT")" == "tsz" \
  && "$(basename "$(dirname "$REPO_PARENT")")" == "worktrees" ]]; then
  WORKTREE_PARENT="$(dirname "$REPO_PARENT")"
fi

MIN_FREE_GB="${TSZ_DISK_MIN_FREE_GB:-20}"
INACTIVE_HOURS="${TSZ_WORKTREE_INACTIVE_HOURS:-4}"
AUTO_PRUNE=false
JSON_REPORT=""

usage() {
  cat <<'EOF'
Compact disk/worktree guard for agents.

This intentionally avoids broad `du` reports. Use it before creating a new
worktree or before starting a large build when disk pressure is suspected.

Usage:
  scripts/setup/disk-worktree-guard.sh
  scripts/setup/disk-worktree-guard.sh --json-report /tmp/tsz-disk-guard.json
  scripts/setup/disk-worktree-guard.sh --auto-prune

Environment:
  TSZ_DISK_MIN_FREE_GB        minimum free space before warning/pruning (default: 20)
  TSZ_WORKTREE_INACTIVE_HOURS minimum age for reuse candidates (default: 4)
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --auto-prune) AUTO_PRUNE=true; shift ;;
    --json-report)
      shift
      if [[ $# -eq 0 ]]; then
        echo "--json-report requires a path (try --help)" >&2
        exit 2
      fi
      JSON_REPORT="$1"
      shift
      ;;
    --json-report=*)
      JSON_REPORT="${1#--json-report=}"
      if [[ -z "$JSON_REPORT" ]]; then
        echo "--json-report requires a path (try --help)" >&2
        exit 2
      fi
      shift
      ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1 (try --help)" >&2; exit 2 ;;
  esac
done

df_kb="$(df -Pk "$WORKTREE_PARENT" | awk 'NR==2 {print $4}')"
free_mb=$(( df_kb / 1024 ))
free_gb=$(( free_mb / 1024 ))
min_free_mb=$(( MIN_FREE_GB * 1024 ))

printf 'disk_free_gb=%s path=%s\n' "$free_gb" "$WORKTREE_PARENT"
printf 'disk_free_mb=%s\n' "$free_mb"

disk_status=ok
disk_shortfall_mb=0
pruned=""
disk_free_gb_after=""
disk_free_mb_after=""
disk_shortfall_mb_after=""
cache_pressure_candidates=""

prune_incremental() {
  local pruned=0
  while IFS= read -r wt; do
    [[ "$wt" == "$WORKTREE_PARENT"/* ]] || continue
    for tdir in "$wt/target" "$wt/.target" "$wt/.target-bench"; do
      [[ -d "$tdir" ]] || continue
      while IFS= read -r inc; do
        stale="$(
          find "$inc" -mindepth 1 -maxdepth 1 -type d -mtime +7 \
            -print -quit 2>/dev/null || true
        )"
        [[ -n "$stale" ]] || continue
        find "$inc" -mindepth 1 -maxdepth 1 -type d -mtime +7 \
          -exec rm -rf {} + 2>/dev/null || true
        pruned=1
      done < <(find "$tdir" -type d -name incremental -mindepth 2 -maxdepth 4 2>/dev/null)
    done
  done < <(git -C "$REPO_ROOT" worktree list --porcelain | awk '/^worktree / {print substr($0, 10)}')
  [[ "$pruned" -eq 1 ]] && echo "pruned=old-cargo-incremental" || echo "pruned=none"
}

if (( free_gb < MIN_FREE_GB )); then
  disk_status=low
  disk_shortfall_mb="$(( min_free_mb > free_mb ? min_free_mb - free_mb : 0 ))"
  printf 'disk_status=low min_free_gb=%s\n' "$MIN_FREE_GB"
  printf 'disk_shortfall_mb=%s\n' "$disk_shortfall_mb"
  if [[ "$AUTO_PRUNE" == true ]]; then
    pruned="$(prune_incremental)"
    echo "$pruned"
    df_kb="$(df -Pk "$WORKTREE_PARENT" | awk 'NR==2 {print $4}')"
    free_mb=$(( df_kb / 1024 ))
    free_gb=$(( free_mb / 1024 ))
    disk_free_gb_after="$free_gb"
    disk_free_mb_after="$free_mb"
    disk_shortfall_mb_after="$(( min_free_mb > free_mb ? min_free_mb - free_mb : 0 ))"
    printf 'disk_free_gb_after=%s\n' "$disk_free_gb_after"
    printf 'disk_free_mb_after=%s\n' "$disk_free_mb_after"
    printf 'disk_shortfall_mb_after=%s\n' "$disk_shortfall_mb_after"
  fi
else
  printf 'disk_status=ok min_free_gb=%s\n' "$MIN_FREE_GB"
fi

cutoff_minutes=$(( INACTIVE_HOURS * 60 ))
echo "sister_worktree_reuse_candidates:"

reuse_candidates="$(
  git -C "$REPO_ROOT" worktree list --porcelain \
    | awk '
      /^worktree / { if (path) print path "\t" branch; path=substr($0,10); branch=""; head="" }
      /^HEAD / { head=substr($0,6) }
      /^branch / { branch=substr($0,8) }
      /^detached/ {
        rev=substr($0,10)
        if (rev == "") rev=substr(head,1,12)
        branch="detached:" rev
      }
      END { if (path) print path "\t" branch }
    ' \
    | while IFS=$'\t' read -r wt branch; do
        [[ "$wt" == "$WORKTREE_PARENT"/* ]] || continue
        [[ "$wt" != "$REPO_ROOT" ]] || continue
        [[ -d "$wt" ]] || continue

        dirty="$(
          git -C "$wt" status --porcelain --untracked-files=normal 2>/dev/null \
            || printf '__status_failed__\n'
        )"
        [[ -z "$dirty" ]] || continue

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
)"

if [[ -n "$reuse_candidates" ]]; then
  printf '%s\n' "$reuse_candidates"
else
  echo "  none"
fi

emit_cache_candidate() {
  local scope="$1"
  local wt="$2"
  local branch="$3"
  local cache_name
  local tdir
  local size_kb
  local size_mb

  for cache_name in target .target .target-bench; do
    tdir="$wt/$cache_name"
    [[ -d "$tdir" ]] || continue
    size_kb="$(du -sk "$tdir" 2>/dev/null | awk 'NR==1 {print $1}')"
    [[ -n "$size_kb" ]] || continue
    (( size_kb > 0 )) || continue
    size_mb=$(( (size_kb + 1023) / 1024 ))
    printf '%012d\tsize_mb=%s path=%s cache=%s scope=%s branch=%s\n' \
      "$size_kb" "$size_mb" "$tdir" "$cache_name" "$scope" "${branch:-unknown}"
  done
}

if [[ "$disk_status" == "low" ]]; then
  echo "cache_pressure_candidates:"
  cache_pressure_candidates="$(
    {
      current_branch="$(
        git -C "$REPO_ROOT" symbolic-ref --short -q HEAD \
          || git -C "$REPO_ROOT" rev-parse --short=12 HEAD
      )"
      emit_cache_candidate current "$REPO_ROOT" "$current_branch"

      if [[ -n "$reuse_candidates" ]]; then
        while IFS= read -r line; do
          line="${line#  }"
          [[ -n "$line" ]] || continue
          [[ "$line" != "none" ]] || continue
          wt="${line%% branch=*}"
          rest="${line#* branch=}"
          branch="${rest%% inactive_hours>=*}"
          [[ -n "$wt" && "$wt" != "$line" ]] || continue
          emit_cache_candidate inactive-clean "$wt" "$branch"
        done <<< "$reuse_candidates"
      fi
    } | sort -rn | head -8 | cut -f2-
  )"

  if [[ -n "$cache_pressure_candidates" ]]; then
    while IFS= read -r line; do
      printf '  %s\n' "$line"
    done <<< "$cache_pressure_candidates"
  else
    echo "  none"
  fi
fi

if [[ -n "$JSON_REPORT" ]]; then
  REUSE_CANDIDATES="$reuse_candidates" \
  CACHE_PRESSURE_CANDIDATES="$cache_pressure_candidates" \
  JSON_REPORT="$JSON_REPORT" \
  WORKTREE_PARENT="$WORKTREE_PARENT" \
  REPO_ROOT="$REPO_ROOT" \
  MIN_FREE_GB="$MIN_FREE_GB" \
  MIN_FREE_MB="$min_free_mb" \
  DISK_FREE_GB="$free_gb" \
  DISK_FREE_MB="$free_mb" \
  DISK_STATUS="$disk_status" \
  DISK_SHORTFALL_MB="$disk_shortfall_mb" \
  AUTO_PRUNE="$AUTO_PRUNE" \
  PRUNED="$pruned" \
  DISK_FREE_GB_AFTER="$disk_free_gb_after" \
  DISK_FREE_MB_AFTER="$disk_free_mb_after" \
  DISK_SHORTFALL_MB_AFTER="$disk_shortfall_mb_after" \
  node <<'NODE'
const fs = require("fs");
const path = require("path");

function numberFromEnv(name) {
  const value = process.env[name];
  if (value == null || value === "") return null;
  return Number(value);
}

const candidates = (process.env.REUSE_CANDIDATES ?? "")
  .split(/\n/)
  .map((line) => line.trim())
  .filter(Boolean)
  .map((line) => {
    const match = line.match(/^(.*) branch=(\S+) inactive_hours>=(\d+)$/);
    if (!match) return { raw: line };
    return {
      path: match[1],
      branch: match[2],
      inactive_hours_min: Number(match[3]),
    };
  });

const cachePressureCandidates = (process.env.CACHE_PRESSURE_CANDIDATES ?? "")
  .split(/\n/)
  .map((line) => line.trim())
  .filter(Boolean)
  .map((line) => {
    const match = /^size_mb=(\d+) path=(.*?) cache=(\S+) scope=(\S+) branch=(.*)$/.exec(line);
    if (!match) return { raw: line };
    return {
      size_mb: Number(match[1]),
      path: match[2],
      cache: match[3],
      scope: match[4],
      branch: match[5],
    };
  });

const report = {
  ok: process.env.DISK_STATUS === "ok",
  status: process.env.DISK_STATUS,
  generated_by: "scripts/setup/disk-worktree-guard.sh",
  repo_root: process.env.REPO_ROOT,
  worktree_parent: process.env.WORKTREE_PARENT,
  min_free_gb: numberFromEnv("MIN_FREE_GB"),
  min_free_mb: numberFromEnv("MIN_FREE_MB"),
  disk_free_gb: numberFromEnv("DISK_FREE_GB"),
  disk_free_mb: numberFromEnv("DISK_FREE_MB"),
  disk_shortfall_mb: numberFromEnv("DISK_SHORTFALL_MB"),
  auto_prune: process.env.AUTO_PRUNE === "true",
  pruned: process.env.PRUNED ? process.env.PRUNED.replace(/^pruned=/, "") : null,
  disk_after_auto_prune: process.env.DISK_FREE_MB_AFTER
    ? {
        disk_free_gb: numberFromEnv("DISK_FREE_GB_AFTER"),
        disk_free_mb: numberFromEnv("DISK_FREE_MB_AFTER"),
        disk_shortfall_mb: numberFromEnv("DISK_SHORTFALL_MB_AFTER"),
      }
    : null,
  reuse_candidate_count: candidates.length,
  reuse_candidates: candidates,
  cache_pressure_candidate_count: cachePressureCandidates.length,
  cache_pressure_candidates: cachePressureCandidates,
};

fs.mkdirSync(path.dirname(process.env.JSON_REPORT), { recursive: true });
fs.writeFileSync(process.env.JSON_REPORT, `${JSON.stringify(report, null, 2)}\n`);
NODE
fi
