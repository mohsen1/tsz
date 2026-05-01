#!/usr/bin/env bash
# =============================================================================
# random-failure-pick.sh - Pick a random conformance failure to work on
# =============================================================================
#
# A small wrapper that picks one truly random failure from the cached
# conformance-detail.json snapshot. Use it to grab a target without
# having to remember pick.py subcommand syntax.
#
# Usage:
#   scripts/session/random-failure-pick.sh
#   scripts/session/random-failure-pick.sh --code TS2322
#   scripts/session/random-failure-pick.sh --seed 42
#   scripts/session/random-failure-pick.sh --run     # also run --verbose
#   scripts/session/random-failure-pick.sh --show    # also print test source
#
# Honours the agent rule: "Take what the picker gives you. Do not reroll."
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
DETAIL="$ROOT/scripts/conformance/conformance-detail.json"

if [[ ! -d "$ROOT/TypeScript/tests" ]]; then
    echo "TypeScript submodule missing - initializing..." >&2
    git -C "$ROOT" submodule update --init TypeScript >&2 || \
        (cd "$ROOT/TypeScript" && \
            git fetch --depth 1 origin "$(git -C "$ROOT" ls-tree HEAD TypeScript | awk '{print $3}')" && \
            git checkout FETCH_HEAD)
fi

if [[ ! -f "$DETAIL" ]]; then
    echo "error: $DETAIL missing." >&2
    echo "  run: scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot" >&2
    exit 1
fi

mode="quick"
for arg in "$@"; do
    case "$arg" in
        --show) mode="show"; shift; break ;;
    esac
done

exec "$SCRIPT_DIR/pick.py" "$mode" "$@"
