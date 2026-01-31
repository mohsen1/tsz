#!/bin/bash
#
# Install git hooks for Project Zang
#
# This script configures git to use the .githooks directory for hooks.
# The hooks run formatting, clippy, and unit tests before commits.
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
GITHOOKS_DIR="$ROOT_DIR/.githooks"

echo "Installing git hooks..."

# Check if .githooks directory exists
if [ ! -d "$GITHOOKS_DIR" ]; then
    echo "Error: .githooks directory not found"
    exit 1
fi

# Check if we're in a git repository
if ! git -C "$ROOT_DIR" rev-parse --git-dir > /dev/null 2>&1; then
    echo "Error: Not a git repository"
    exit 1
fi

# Configure git to use .githooks directory
# This is the modern approach - no symlinks needed
git -C "$ROOT_DIR" config core.hooksPath .githooks

# Make all hooks executable
chmod +x "$GITHOOKS_DIR"/* 2>/dev/null || true

echo "Git hooks installed successfully!"
echo ""
echo "The following hooks are now active:"
for hook in "$GITHOOKS_DIR"/*; do
    if [ -f "$hook" ]; then
        hook_name=$(basename "$hook")
        echo "  - $hook_name"
    fi
done
echo ""
echo "Hook features:"
echo "  - pre-commit: Format, clippy, tests, conformance regression check"
echo "  - prepare-commit-msg: Auto-append conformance % to commit messages"
echo ""
echo "To disable hooks temporarily, use: git commit --no-verify"
echo "To skip conformance check only: TSZ_SKIP_CONFORMANCE=1 git commit ..."
