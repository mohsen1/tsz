#!/usr/bin/env bash
# =============================================================================
# pick-random-failure.sh — One-shot "give me a random conformance failure to fix"
# =============================================================================
#
# Usage:
#   scripts/session/pick-random-failure.sh                     # any category
#   scripts/session/pick-random-failure.sh --fingerprint-only  # Tier 1 target
#   scripts/session/pick-random-failure.sh --wrong-code        # Tier 2 target
#   scripts/session/pick-random-failure.sh --code TS2322       # specific code
#   scripts/session/pick-random-failure.sh --one-extra         # leaf fix
#   scripts/session/pick-random-failure.sh --one-missing       # leaf fix
#   scripts/session/pick-random-failure.sh --seed 42           # reproducible
#   scripts/session/pick-random-failure.sh --run               # also run it
#
# What it does:
#   1. Ensures the TypeScript submodule is initialized.
#   2. Picks a random failure from conformance-detail.json using the Python
#      picker, forwarding any remaining filter flags.
#   3. Prints the target and, with --run, runs it through the conformance
#      runner in --verbose mode so you see the fingerprint diff immediately.
#
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
PICKER="$SCRIPT_DIR/pick-random-failure.py"

if [[ -t 1 ]]; then
    BOLD='\033[1m' CYAN='\033[0;36m' GREEN='\033[0;32m' YELLOW='\033[0;33m' RESET='\033[0m'
else
    BOLD='' CYAN='' GREEN='' YELLOW='' RESET=''
fi

RUN_AFTER=false
FORWARD_ARGS=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --run) RUN_AFTER=true; shift ;;
        # Category shortcuts — translate to the picker's --category flag.
        --fingerprint-only|--wrong-code|--only-missing|--only-extra|--all-missing|--false-positive)
            FORWARD_ARGS+=(--category "${1#--}"); shift ;;
        -h|--help)
            sed -n '2,30p' "$0"
            echo
            echo "Forwarded to pick-random-failure.py:"
            python3 "$PICKER" --help | sed 's/^/  /'
            exit 0
            ;;
        *) FORWARD_ARGS+=("$1"); shift ;;
    esac
done

# --- 1. Ensure TypeScript submodule is initialized ---
if [[ ! -d "$REPO_ROOT/TypeScript/tests" ]]; then
    echo -e "${YELLOW}!${RESET} TypeScript submodule missing — initializing…"
    git -C "$REPO_ROOT" submodule update --init --depth 1 TypeScript
fi

# --- 2. Ensure conformance snapshot exists ---
DETAIL="$REPO_ROOT/scripts/conformance/conformance-detail.json"
if [[ ! -f "$DETAIL" ]]; then
    echo -e "${YELLOW}!${RESET} Conformance detail missing. Run:"
    echo "    scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot"
    exit 1
fi

# --- 3. Pick a random failure ---
echo -e "${CYAN}${BOLD}Picking random failure…${RESET}"
PICK_OUTPUT="$(python3 "$PICKER" "${FORWARD_ARGS[@]}")"
echo "$PICK_OUTPUT"

if ! $RUN_AFTER; then
    echo
    echo -e "${CYAN}tip:${RESET} rerun with --run to execute it through conformance.sh --verbose"
    exit 0
fi

# --- 4. Run the picked test through the conformance runner ---
TARGET_PATH="$(echo "$PICK_OUTPUT" | awk '/^path: /{print $2; exit}')"
if [[ -z "$TARGET_PATH" ]]; then
    echo "error: could not parse picked test path" >&2
    exit 1
fi

# conformance.sh --filter matches on the test name (basename without extension)
FILTER="$(basename "$TARGET_PATH")"
FILTER="${FILTER%.*}"

echo
echo -e "${CYAN}${BOLD}Running conformance with --verbose for: ${GREEN}$FILTER${RESET}"
exec "$REPO_ROOT/scripts/conformance/conformance.sh" run --filter "$FILTER" --verbose
