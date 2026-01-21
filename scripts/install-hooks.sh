#!/bin/bash
#
# Install git hooks from .githooks directory
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
GITHOOKS_DIR="$ROOT_DIR/.githooks"
GIT_HOOKS_DIR="$ROOT_DIR/.git/hooks"

echo "📦 Installing git hooks..."

# Check if .githooks directory exists
if [ ! -d "$GITHOOKS_DIR" ]; then
    echo "❌ .githooks directory not found"
    exit 1
fi

# Create symlink for each hook
for hook in "$GITHOOKS_DIR"/*; do
    hook_name=$(basename "$hook")
    target="$GIT_HOOKS_DIR/$hook_name"

    # Remove existing hook
    if [ -e "$target" ]; then
        rm "$target"
    fi

    # Create symlink
    ln -s "$GITHOOKS_DIR/$hook_name" "$target"
    echo "✅ Installed $hook_name"
done

echo ""
echo "✅ Git hooks installed successfully!"
