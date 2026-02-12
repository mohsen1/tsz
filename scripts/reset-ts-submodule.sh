#!/bin/bash
# Reset TypeScript submodule to the pinned SHA from typescript-versions.json
#
# Usage: ./scripts/reset-ts-submodule.sh
#
# Reads "current" SHA from scripts/typescript-versions.json and ensures
# the TypeScript submodule is checked out at that exact commit.

set -e

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSIONS_FILE="$ROOT_DIR/scripts/typescript-versions.json"

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

# Get actual submodule HEAD
ACTUAL_SHA=$(git -C "$ROOT_DIR/TypeScript" rev-parse HEAD 2>/dev/null || echo "")

if [ "$ACTUAL_SHA" = "$PINNED_SHA" ]; then
    exit 0  # already correct, nothing to do
fi

echo "Resetting TypeScript submodule to pinned SHA: ${PINNED_SHA:0:12}..."
cd "$ROOT_DIR"
git submodule update --init --force -- TypeScript
git -C TypeScript checkout "$PINNED_SHA" --quiet
echo "TypeScript submodule reset to $PINNED_SHA"
