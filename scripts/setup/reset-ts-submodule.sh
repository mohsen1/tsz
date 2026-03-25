#!/bin/bash
# Reset TypeScript submodule to the pinned SHA from typescript-versions.json
#
# Usage: ./scripts/setup/reset-ts-submodule.sh
#
# Reads "current" SHA from scripts/conformance/typescript-versions.json and ensures
# the TypeScript submodule is checked out at that exact commit.

set -e

# Unset git environment variables that hooks inherit — they interfere
# with submodule operations by overriding gitlink resolution.
unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")"/../.. && pwd)"
VERSIONS_FILE="$ROOT_DIR/scripts/conformance/typescript-versions.json"
SUBMODULE_PATH="$ROOT_DIR/TypeScript"
SUBMODULE_GIT_DIR="$(git -C "$ROOT_DIR" rev-parse --git-path modules/TypeScript)"
SUBMODULE_URL="$(git -C "$ROOT_DIR" config -f .gitmodules --get submodule.TypeScript.url)"

if [ ! -f "$VERSIONS_FILE" ]; then
    echo "ERROR: $VERSIONS_FILE not found"
    exit 1
fi

# Extract the "current" SHA (portable: no jq dependency)
PINNED_SHA=$(grep '"current"' "$VERSIONS_FILE" | head -1 | sed 's/.*: *"\([a-f0-9]*\)".*/\1/')

if [ -z "$PINNED_SHA" ]; then
    echo "ERROR: Could not read 'current' SHA from $VERSIONS_FILE"
    exit 1
fi

# Get actual submodule HEAD and worktree state
ACTUAL_SHA=$(git -C "$SUBMODULE_PATH" rev-parse HEAD 2>/dev/null || echo "")
DIRTY_STATE=$(git -C "$SUBMODULE_PATH" status --porcelain 2>/dev/null || true)

if [ "$ACTUAL_SHA" = "$PINNED_SHA" ] && [ -z "$DIRTY_STATE" ]; then
    exit 0  # already correct and clean, nothing to do
fi

echo "Resetting TypeScript submodule to pinned SHA: ${PINNED_SHA:0:12}..."
cd "$ROOT_DIR"

reclone_submodule() {
    rm -rf "$SUBMODULE_PATH" "$SUBMODULE_GIT_DIR"
    mkdir -p "$(dirname "$SUBMODULE_GIT_DIR")"
    git clone --no-checkout --separate-git-dir "$SUBMODULE_GIT_DIR" "$SUBMODULE_URL" "$SUBMODULE_PATH"
}

if [ -d "$SUBMODULE_PATH" ] && ! git -C "$SUBMODULE_PATH" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    rm -rf "$SUBMODULE_PATH"
fi

# Fresh worktrees can leave the submodule gitdir on an unborn/invalid HEAD
# (for example "ref: refs/heads/.invalid"), which causes submodule update to
# fail before it can materialize the pinned commit. Deinit first to recover.
if [ -z "$ACTUAL_SHA" ] || [ "$ACTUAL_SHA" = "HEAD" ]; then
    git submodule deinit -f -- TypeScript >/dev/null 2>&1 || true
fi

git config submodule.TypeScript.shallow true
if ! git submodule update --init --depth 1 --force -- TypeScript; then
    git submodule deinit -f -- TypeScript >/dev/null 2>&1 || true
    git config submodule.TypeScript.shallow false
    if ! git submodule update --init --force -- TypeScript; then
        reclone_submodule
    fi
fi
if ! git -C "$SUBMODULE_PATH" checkout "$PINNED_SHA" --quiet; then
    git -C "$SUBMODULE_PATH" fetch --quiet origin "$PINNED_SHA" || {
        reclone_submodule
        git -C "$SUBMODULE_PATH" fetch --quiet origin "$PINNED_SHA"
    }
    git -C "$SUBMODULE_PATH" checkout "$PINNED_SHA" --quiet
fi
git -C "$SUBMODULE_PATH" reset --hard --quiet
git -C "$SUBMODULE_PATH" checkout -- .
if [ "$(git -C "$SUBMODULE_PATH" config --get core.sparseCheckout 2>/dev/null || echo "false")" = "true" ]; then
    git -C "$SUBMODULE_PATH" sparse-checkout reapply
fi
git -C "$SUBMODULE_PATH" clean -fd --quiet
echo "TypeScript submodule reset to $PINNED_SHA"
