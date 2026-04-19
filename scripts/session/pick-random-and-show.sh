#!/usr/bin/env bash
# pick-random-and-show.sh — Pick a random conformance failure and show the diff.
#
# Quick workflow tool: combines pick-random-failure.py with conformance.sh
# verbose runner so you can immediately see expected vs actual diagnostics.
#
# Usage:
#   scripts/session/pick-random-and-show.sh                  # any failure
#   scripts/session/pick-random-and-show.sh fingerprint-only # by category
#   scripts/session/pick-random-and-show.sh wrong-code TS2322
#   scripts/session/pick-random-and-show.sh any "" 42        # with seed
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
cd "$REPO_ROOT"

CATEGORY="${1:-any}"
CODE="${2:-}"
SEED="${3:-}"

ARGS=(--category "$CATEGORY")
[[ -n "$CODE" ]] && ARGS+=(--code "$CODE")
[[ -n "$SEED" ]] && ARGS+=(--seed "$SEED")

PICK_OUT=$(python3 scripts/session/pick-random-failure.py "${ARGS[@]}")
echo "$PICK_OUT"

PATH_LINE=$(printf '%s\n' "$PICK_OUT" | awk -F': *' '/^path:/ {print $2; exit}')
if [[ -z "$PATH_LINE" ]]; then
    echo "no path returned from picker" >&2
    exit 1
fi

BASENAME=$(basename "$PATH_LINE" .ts)
BASENAME=$(basename "$BASENAME" .tsx)

echo "──── running conformance for: $BASENAME ────"
./scripts/conformance/conformance.sh run --filter "$BASENAME" --verbose || true
