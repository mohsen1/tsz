#!/usr/bin/env bash
# =============================================================================
# pick-and-run-random.sh — Pick a random conformance failure and run it verbose
# =============================================================================
#
# One-shot helper for an agent starting a new unit of work. Picks a random
# failing test from conformance-detail.json, prints its expected/actual
# diagnostic summary, and then runs the conformance harness on just that test
# with --verbose so fingerprint deltas are visible immediately.
#
# Usage:
#   scripts/session/pick-and-run-random.sh
#   scripts/session/pick-and-run-random.sh --category fingerprint-only
#   scripts/session/pick-and-run-random.sh --code TS2322
#   scripts/session/pick-and-run-random.sh --seed 42
#   scripts/session/pick-and-run-random.sh --no-run     # just pick, don't run
#
# All unrecognized flags are forwarded to pick-random-failure.py, so filters
# like --one-missing, --close N, --extra-code TS7053 work too.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
PICKER="$REPO_ROOT/scripts/session/pick-random-failure.py"
CONFORMANCE="$REPO_ROOT/scripts/conformance/conformance.sh"

if [[ -t 1 ]]; then
    BOLD='\033[1m' CYAN='\033[0;36m' YELLOW='\033[0;33m' RESET='\033[0m'
else
    BOLD='' CYAN='' YELLOW='' RESET=''
fi

RUN=true
PICKER_ARGS=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --no-run) RUN=false; shift ;;
        --help|-h)
            sed -n '2,20p' "$0"
            exit 0
            ;;
        *) PICKER_ARGS+=("$1"); shift ;;
    esac
done

if [[ ! -x "$PICKER" ]] && [[ ! -f "$PICKER" ]]; then
    echo "error: picker script not found at $PICKER" >&2
    exit 1
fi

echo -e "${BOLD}${CYAN}━━━ Picking a random failure ━━━${RESET}"
PICK_OUTPUT="$(python3 "$PICKER" --count 1 "${PICKER_ARGS[@]}")"
echo "$PICK_OUTPUT"

TEST_PATH="$(printf '%s\n' "$PICK_OUTPUT" | awk -F': *' '/^path: /{print $2; exit}')"
if [[ -z "$TEST_PATH" ]]; then
    echo "error: could not extract test path from picker output" >&2
    exit 1
fi

TEST_BASENAME="$(basename "$TEST_PATH")"
TEST_STEM="${TEST_BASENAME%.ts}"
TEST_STEM="${TEST_STEM%.tsx}"

echo ""
echo -e "${BOLD}${CYAN}━━━ Test: ${TEST_BASENAME} ━━━${RESET}"
echo -e "${YELLOW}Filter pattern:${RESET} ${TEST_STEM}"
echo ""

if ! $RUN; then
    cat <<EOF
To run this test verbose:
  ${CONFORMANCE} run --filter "${TEST_STEM}" --verbose

To inspect the source:
  cat "${REPO_ROOT}/${TEST_PATH}"
EOF
    exit 0
fi

echo -e "${BOLD}${CYAN}━━━ Running conformance with --verbose ━━━${RESET}"
exec "$CONFORMANCE" run --filter "$TEST_STEM" --verbose
