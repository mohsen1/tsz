#!/bin/bash
#
# Install git hooks for Project Zang
#
# This script configures git to use the scripts/githooks directory for hooks.
# The hooks run formatting, clippy, and unit tests before commits.
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
GITHOOKS_DIR="$ROOT_DIR/scripts/githooks"
GIT_HOOKS_DIR="$ROOT_DIR/.git/hooks"

echo "Installing git hooks..."

# Check if scripts/githooks directory exists
if [ ! -d "$GITHOOKS_DIR" ]; then
    echo "Error: scripts/githooks directory not found"
    exit 1
fi

# Check if we're in a git repository
if ! git -C "$ROOT_DIR" rev-parse --git-dir > /dev/null 2>&1; then
    echo "Error: Not a git repository"
    exit 1
fi

# Clean up any stale symlinks or old hooks in .git/hooks/ that might conflict
# This is important when core.hooksPath is set - old symlinks can cause issues
if [ -d "$GIT_HOOKS_DIR" ]; then
    echo "Cleaning up stale hooks in .git/hooks/..."
    for hook in pre-commit prepare-commit-msg commit-msg post-commit pre-push; do
        hook_path="$GIT_HOOKS_DIR/$hook"
        if [ -L "$hook_path" ]; then
            echo "  Removing stale symlink: $hook"
            rm -f "$hook_path"
        elif [ -f "$hook_path" ] && [ ! -f "$hook_path.sample" ]; then
            # It's a real file (not a .sample), back it up
            if ! grep -q "scripts/githooks" "$hook_path" 2>/dev/null; then
                echo "  Backing up old hook: $hook -> $hook.bak"
                mv "$hook_path" "$hook_path.bak"
            fi
        fi
    done
fi

# Configure git to use scripts/githooks directory
# This is the modern approach - no symlinks needed
git -C "$ROOT_DIR" config core.hooksPath scripts/githooks

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
