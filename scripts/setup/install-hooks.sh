#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"

CURRENT_HOOKS_PATH=$(git -C "$ROOT_DIR" config --get core.hooksPath 2>/dev/null || true)
if [[ "$CURRENT_HOOKS_PATH" != "scripts/githooks" ]]; then
    git -C "$ROOT_DIR" config core.hooksPath scripts/githooks
fi

chmod +x "$ROOT_DIR/scripts/githooks"/* 2>/dev/null || true
