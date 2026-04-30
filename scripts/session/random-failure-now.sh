#!/usr/bin/env bash
# =============================================================================
# random-failure-now.sh — One-liner random conformance failure picker
# =============================================================================
#
# Quick wrapper that:
#   1. Ensures the TypeScript submodule is initialized.
#   2. Ensures conformance-detail.json exists.
#   3. Picks a random failure (delegates to scripts/session/pick.py).
#
# This is a thin, no-arg wrapper for "give me something to work on right now".
# For seed/code/category control, use scripts/session/quick-pick.sh instead.
#
# Usage:
#   scripts/session/random-failure-now.sh
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"

# 1) TypeScript submodule
if [[ ! -d "$REPO_ROOT/TypeScript/tests" ]]; then
    echo "TypeScript submodule missing — initializing (depth 1)..." >&2
    git -C "$REPO_ROOT" submodule update --init --depth 1 TypeScript >&2
fi

# 2) conformance-detail.json
if [[ ! -f "$REPO_ROOT/scripts/conformance/conformance-detail.json" ]]; then
    echo "conformance-detail.json missing — refreshing snapshot..." >&2
    "$REPO_ROOT/scripts/safe-run.sh" \
        "$REPO_ROOT/scripts/conformance/conformance.sh" snapshot >&2
fi

# 3) Random pick — uses a fresh RNG seed every call.
exec "$SCRIPT_DIR/pick.py" quick "$@"
