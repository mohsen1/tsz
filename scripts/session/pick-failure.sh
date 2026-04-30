#!/usr/bin/env bash
# =============================================================================
# pick-failure.sh — One-shot random conformance failure picker for agents.
# =============================================================================
#
# Self-contained wrapper that:
#   1. Initialises the TypeScript submodule (depth 1) if it isn't already.
#   2. Refreshes scripts/conformance/conformance-detail.json if missing.
#   3. Picks ONE random failing conformance test and prints:
#        - the source path,
#        - failure category (fingerprint-only / one-missing / wrong-code / ...),
#        - expected/actual/missing/extra error codes,
#        - the candidate pool size,
#        - a ready-to-paste verbose-run command.
#
# Usage:
#   scripts/session/pick-failure.sh                 # any failure
#   scripts/session/pick-failure.sh --seed 42       # reproducible pick
#   scripts/session/pick-failure.sh --code TS2322   # restrict to one error code
#   scripts/session/pick-failure.sh --run           # also run conformance --verbose
#
# Selection logic is delegated to scripts/session/pick.py so the rules stay
# aligned with the rest of the session toolkit. Use this script as the entry
# point that "always works" — it sets up the submodule and snapshot for you.
#
# See scripts/session/conformance-agent-prompt.md for the full agent process.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"

# 1. TypeScript submodule (required for the verbose-run command to find the test).
if [[ ! -d "$REPO_ROOT/TypeScript/tests" ]]; then
    echo "TypeScript submodule missing — initializing (depth 1)..." >&2
    git -C "$REPO_ROOT" submodule update --init --depth 1 TypeScript >&2
fi

# 2. Offline failure snapshot (the picker reads this; never the live suite).
SNAPSHOT="$REPO_ROOT/scripts/conformance/conformance-detail.json"
if [[ ! -f "$SNAPSHOT" ]]; then
    echo "conformance-detail.json missing — refreshing snapshot via safe-run..." >&2
    "$REPO_ROOT/scripts/safe-run.sh" \
        "$REPO_ROOT/scripts/conformance/conformance.sh" snapshot >&2
fi

# 3. Random pick (delegates to the canonical picker).
exec "$SCRIPT_DIR/pick.py" quick "$@"
