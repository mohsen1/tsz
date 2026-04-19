#!/usr/bin/env bash
# Pick a random conformance failure and dump everything needed to start work:
# the chosen test path, its expected/actual/missing/extra codes, the source
# snippet, and (optionally) a verbose conformance run for fingerprint diffs.
#
# Usage:
#   scripts/session/pick-and-prepare.sh                      # random any category
#   scripts/session/pick-and-prepare.sh --category wrong-code
#   scripts/session/pick-and-prepare.sh --code TS2322
#   scripts/session/pick-and-prepare.sh --seed 42 --no-run   # deterministic, skip run
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"

RUN_TEST=true
PICK_ARGS=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --no-run) RUN_TEST=false; shift ;;
        *) PICK_ARGS+=("$1"); shift ;;
    esac
done

cd "$REPO_ROOT"

# 1. Pick a single random failure.
PICK_OUTPUT="$(python3 scripts/session/pick-random-failure.py --count 1 "${PICK_ARGS[@]}" 2>/dev/null)"
if [[ -z "$PICK_OUTPUT" ]]; then
    echo "no failures matched the given filters" >&2
    exit 1
fi

printf '%s\n\n' "$PICK_OUTPUT"

TEST_PATH="$(printf '%s\n' "$PICK_OUTPUT" | awk '/^path: /{print $2; exit}')"
if [[ -z "$TEST_PATH" ]]; then
    echo "could not parse test path from picker output" >&2
    exit 1
fi

# 2. Print the source for quick context.
SRC="$REPO_ROOT/$TEST_PATH"
echo "---- source ($SRC) ----"
if [[ -f "$SRC" ]]; then
    head -80 "$SRC"
else
    echo "source file not found (TypeScript submodule may not be checked out)"
fi
echo

# 3. Optionally run the test with --verbose to see the fingerprint diff.
if $RUN_TEST; then
    FILTER="$(basename "$TEST_PATH" .ts)"
    FILTER="${FILTER%.tsx}"
    echo "---- running: conformance.sh run --filter \"$FILTER\" --verbose ----"
    ./scripts/conformance/conformance.sh run --filter "$FILTER" --verbose 2>&1 | tail -60
fi
