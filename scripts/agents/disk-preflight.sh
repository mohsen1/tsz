#!/usr/bin/env bash
#
# Fast preflight for multi-agent work. It reports disk guard state, reusable
# worktrees, TypeScript submodule linkage, and cache-preserving cleanup advice.

set -euo pipefail

usage() {
  cat <<'USAGE'
usage: scripts/agents/disk-preflight.sh [AgentName]

Runs compact checks only. It does not delete files or create worktrees.
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

AGENT="${1:-unknown}"
ROOT="$(git rev-parse --show-toplevel)"

echo "agent=$AGENT"
echo "repo=$ROOT"
echo ""
echo "== disk guard =="
GUARD_OUTPUT="$("$ROOT/scripts/setup/disk-worktree-guard.sh")"
echo "$GUARD_OUTPUT"

echo ""
echo "== current TypeScript state =="
if [[ -L "$ROOT/TypeScript" ]]; then
  echo "typescript=symlink target=$(readlink "$ROOT/TypeScript")"
elif [[ -d "$ROOT/TypeScript/tests/cases" ]]; then
  echo "typescript=populated-local-submodule"
elif [[ -d "$ROOT/TypeScript" ]]; then
  echo "typescript=present-but-not-populated"
else
  echo "typescript=missing"
fi

COMMON_DIR="$(git -C "$ROOT" rev-parse --git-common-dir)"
GIT_DIR="$(git -C "$ROOT" rev-parse --git-dir)"
COMMON_REAL="$(cd "$COMMON_DIR" && pwd -P)"
GIT_REAL="$(cd "$GIT_DIR" && pwd -P)"
PRIMARY_REPO="$(cd "$COMMON_REAL/.." && pwd -P)"
PRIMARY_TS="$PRIMARY_REPO/TypeScript"

echo ""
echo "== local cargo cache presence =="
for dir in .target .target-bench target; do
  if [[ -d "$ROOT/$dir" ]]; then
    echo "$dir=present"
  else
    echo "$dir=missing"
  fi
done

echo ""
echo "== TypeScript reuse sources =="
if [[ -d "$PRIMARY_TS/tests/cases" ]]; then
  echo "primary=$PRIMARY_REPO ts-populated"
else
  echo "primary=$PRIMARY_REPO ts-missing-or-unpopulated"
fi

TS_SOURCE_COUNT=0
while IFS= read -r wt; do
  [[ -n "$wt" ]] || continue
  [[ "$wt" != "$ROOT" ]] || continue
  if [[ -d "$wt/TypeScript/tests/cases" ]]; then
    TS_SOURCE_COUNT=$((TS_SOURCE_COUNT + 1))
    echo "source=$wt"
  fi
done < <(git -C "$ROOT" worktree list --porcelain | awk '/^worktree / { print substr($0, 10) }')

if [[ "$COMMON_REAL" != "$GIT_REAL" && ! -e "$ROOT/TypeScript/tests/cases" ]]; then
  if [[ -d "$PRIMARY_TS/tests/cases" ]]; then
    echo "hint=run scripts/setup/link-ts-submodule.sh"
  elif (( TS_SOURCE_COUNT > 0 )); then
    echo "hint=run scripts/setup/link-ts-submodule.sh --source <source-path-above>"
  else
    echo "hint=no populated TypeScript source found; run scripts/setup/setup-ts-submodule.sh in the primary checkout first"
  fi
fi

echo ""
echo "== reusable worktree signals =="
git -C "$ROOT" worktree list --porcelain \
  | awk '/^worktree / { print substr($0, 10) }' \
  | while IFS= read -r wt; do
      [[ -n "$wt" ]] || continue
      flags=()
      [[ -L "$wt/TypeScript" ]] && flags+=("ts-link")
      [[ -d "$wt/TypeScript/tests/cases" ]] && flags+=("ts-populated")
      [[ -d "$wt/.target" ]] && flags+=(".target")
      [[ -d "$wt/target" ]] && flags+=("target")
      [[ ${#flags[@]} -eq 0 ]] && flags+=("no-local-cache-signal")
      printf "%s %s\n" "$wt" "${flags[*]}"
    done

if echo "$GUARD_OUTPUT" | grep -q 'disk_status=low'; then
  cat <<'LOWDISK'

== low disk cleanup ladder ==
1. Reuse an existing worktree with TypeScript/cache state.
2. Run scripts/setup/disk-worktree-guard.sh --auto-prune.
3. Run scripts/setup/clean.sh --quiet to preserve .target, .target-bench, and target.
4. Delete only abandoned worktrees whose branch/PR owner is understood.
5. Use scripts/setup/clean.sh --full only as a deliberate last resort.
LOWDISK
fi
