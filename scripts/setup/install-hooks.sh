#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage:
  scripts/setup/install-hooks.sh

Configures this repository to use scripts/githooks as its git hooks path and
marks local hook scripts executable.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1 (try --help)" >&2
            exit 1
            ;;
    esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"

CURRENT_HOOKS_PATH=$(git -C "$ROOT_DIR" config --get core.hooksPath 2>/dev/null || true)
if [[ "$CURRENT_HOOKS_PATH" != "scripts/githooks" ]]; then
    git -C "$ROOT_DIR" config core.hooksPath scripts/githooks
fi

chmod +x "$ROOT_DIR/scripts/githooks"/* 2>/dev/null || true
