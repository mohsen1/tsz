#!/usr/bin/env bash
#
# link-ts-submodule.sh — share the TypeScript submodule across worktrees.
#
# When run inside a git worktree (not the primary checkout), replace the
# local TypeScript/ directory with a symlink to a populated TypeScript/
# checkout. By default the source is the primary checkout's TypeScript/, but
# --source can point at another worktree or TypeScript/ directory when the
# primary checkout has not been populated yet. The submodule is pinned to a
# single SHA, so every worktree wants the same content — symlinking avoids
# 250–500 MB of duplicated fixture data per worktree.
#
# Idempotent. No-op when:
#   - this is the primary checkout (not a worktree)
#   - TypeScript/ is already a symlink
#   - the source TypeScript/ is missing or uninitialised
#     (caller should run setup-ts-submodule.sh in the primary first)
#
# Refuses to overwrite a TypeScript/ directory that has local edits
# tracked by its submodule git, to avoid silently losing in-progress work.
#
# Usage:
#   ./scripts/setup/link-ts-submodule.sh           # symlink (default)
#   ./scripts/setup/link-ts-submodule.sh --force   # ignore dirty state
#   ./scripts/setup/link-ts-submodule.sh --source ../tsz-main
#                                                  # link to another populated worktree
#   ./scripts/setup/link-ts-submodule.sh --unlink  # restore real submodule
#   ./scripts/setup/link-ts-submodule.sh --quiet   # suppress info output

set -euo pipefail

# Use the cwd's git toplevel as ROOT_DIR (not the script's location). The
# script is normally invoked by absolute path from a worktree, so $BASH_SOURCE
# points at the primary checkout — that would be the wrong target.
ROOT_DIR="$(git rev-parse --show-toplevel 2>/dev/null || true)"
if [ -z "$ROOT_DIR" ]; then
  echo "[link-ts] not in a git repository — aborting." >&2
  exit 1
fi

FORCE=false
UNLINK=false
QUIET=false
SOURCE_PATH=""

usage() {
  cat <<'USAGE'
Usage:
  scripts/setup/link-ts-submodule.sh           # symlink TypeScript/ to the primary checkout
  scripts/setup/link-ts-submodule.sh --force   # ignore dirty state in the local TypeScript/
  scripts/setup/link-ts-submodule.sh --source <repo-or-TypeScript-dir>
                                                # link to an explicit populated source
  scripts/setup/link-ts-submodule.sh --unlink  # remove the symlink so a real submodule can be restored
  scripts/setup/link-ts-submodule.sh --quiet   # suppress info output
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --force)   FORCE=true; shift ;;
    --unlink)  UNLINK=true; shift ;;
    --quiet)   QUIET=true; shift ;;
    --source)
      if [[ -z "${2:-}" ]]; then
        echo "Missing value for --source (try --help)" >&2
        exit 1
      fi
      SOURCE_PATH="$2"
      shift 2
      ;;
    -h|--help) usage; exit 0 ;;
    *)         echo "Unknown option: $1 (try --help)" >&2; exit 1 ;;
  esac
done

log() { [[ "$QUIET" == true ]] && return; echo "[link-ts] $*"; }

LOCAL_TS="$ROOT_DIR/TypeScript"

# In a worktree, --git-common-dir points to the primary checkout's .git, while
# --git-dir points to .git/worktrees/<name>. They are equal in the primary.
COMMON_DIR="$(git -C "$ROOT_DIR" rev-parse --git-common-dir)"
GIT_DIR="$(git -C "$ROOT_DIR" rev-parse --git-dir)"
if [ "$(cd "$COMMON_DIR" && pwd -P)" = "$(cd "$GIT_DIR" && pwd -P)" ]; then
  log "primary checkout — nothing to link."
  exit 0
fi

# Primary checkout = parent of the common .git directory.
MAIN_REPO="$(cd "$COMMON_DIR/.." && pwd -P)"

resolve_source_ts() {
  local source="$1"
  local source_abs
  if [[ "$source" = /* ]]; then
    source_abs="$source"
  else
    source_abs="$ROOT_DIR/$source"
  fi

  if [[ -d "$source_abs/TypeScript" ]]; then
    cd "$source_abs/TypeScript" && pwd -P
  elif [[ -d "$source_abs" ]]; then
    cd "$source_abs" && pwd -P
  else
    echo "[link-ts] source path does not exist: $source" >&2
    return 1
  fi
}

if [[ -n "$SOURCE_PATH" ]]; then
  MAIN_TS="$(resolve_source_ts "$SOURCE_PATH")"
else
  MAIN_TS="$MAIN_REPO/TypeScript"
fi

suggest_sources() {
  local found=false
  while IFS= read -r wt; do
    [[ -n "$wt" ]] || continue
    [[ "$wt" != "$ROOT_DIR" ]] || continue
    if [[ -d "$wt/TypeScript/tests/cases" ]]; then
      found=true
      echo "          scripts/setup/link-ts-submodule.sh --source \"$wt\""
    fi
  done < <(git -C "$ROOT_DIR" worktree list --porcelain | awk '/^worktree / { print substr($0, 10) }')

  if [[ "$found" != true ]]; then
    echo "          no populated TypeScript/ source found in current worktree list" >&2
  fi
}

# --- Unlink path: restore a real submodule from a symlink ------------------
if [[ "$UNLINK" == true ]]; then
  if [ -L "$LOCAL_TS" ]; then
    rm "$LOCAL_TS"
    log "removed symlink at $LOCAL_TS"
    log "run scripts/setup/reset-ts-submodule.sh to restore the real submodule"
  else
    log "TypeScript is not a symlink — nothing to unlink."
  fi
  exit 0
fi

# --- Validate the source TypeScript ----------------------------------------
if [ ! -d "$MAIN_TS/tests/cases" ] || [ ! -e "$MAIN_TS/.git" ]; then
  echo "[link-ts] source TypeScript missing or uninitialised:" >&2
  echo "          $MAIN_TS" >&2
  if [[ -n "$SOURCE_PATH" ]]; then
    echo "          choose a populated worktree or run scripts/setup/setup-ts-submodule.sh in the source repo" >&2
  else
    echo "          run scripts/setup/setup-ts-submodule.sh in $MAIN_REPO first" >&2
    echo "          or choose a populated worktree source:" >&2
    suggest_sources >&2
  fi
  exit 1
fi

# Already linked?
if [ -L "$LOCAL_TS" ]; then
  current="$(readlink "$LOCAL_TS")"
  if [ "$current" = "$MAIN_TS" ]; then
    log "already linked to $MAIN_TS"
    exit 0
  fi
  log "relinking $LOCAL_TS ($current → $MAIN_TS)"
  rm "$LOCAL_TS"
fi

# --- Refuse to clobber a dirty TypeScript ---------------------------------
if [ -d "$LOCAL_TS" ] && [[ "$FORCE" != true ]]; then
  if git -C "$LOCAL_TS" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    dirty="$(git -C "$LOCAL_TS" status --porcelain 2>/dev/null || true)"
    if [ -n "$dirty" ]; then
      echo "[link-ts] $LOCAL_TS has local edits — refusing to overwrite." >&2
      echo "          commit/stash inside TypeScript/ or pass --force to discard." >&2
      exit 1
    fi
  fi
fi

# Free up the worktree's submodule gitdir so future submodule commands in
# this worktree don't try to manage the now-symlinked path. Safe to ignore
# failures: deinit can't run on already-deinited submodules.
git -C "$ROOT_DIR" submodule deinit -f -- TypeScript >/dev/null 2>&1 || true

# Replace directory with symlink.
[ -d "$LOCAL_TS" ] && rm -rf "$LOCAL_TS"
ln -s "$MAIN_TS" "$LOCAL_TS"

log "linked $LOCAL_TS → $MAIN_TS"
