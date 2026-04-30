#!/usr/bin/env bash
# =============================================================================
# quick-random-failure.sh — One-shot random conformance failure picker
# =============================================================================
#
# Self-contained quick picker: ensures the TypeScript submodule is initialised,
# verifies offline snapshot data, prints one random failing test with its
# expected/actual diagnostic codes, the verbose-run command to repro it, and
# (optionally) a peek at the test source.
#
# Usage:
#   scripts/session/quick-random-failure.sh                # any random failure
#   scripts/session/quick-random-failure.sh --code TS2322  # filter by code
#   scripts/session/quick-random-failure.sh --seed 42      # reproducible pick
#   scripts/session/quick-random-failure.sh --show         # also print source
#   scripts/session/quick-random-failure.sh --run          # run --verbose now
#
# Notes:
#   * Backed by `scripts/session/pick.py` (canonical selection logic).
#   * `quick-pick.sh` is the older equivalent. This wrapper layers in a
#     submodule sanity check + optional source preview so you can start
#     diagnosing with no extra `Read`/`cat` step.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"

SHOW=false
RUN=false
PASSTHRU=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --show) SHOW=true; shift ;;
        --run) RUN=true; PASSTHRU+=("$1"); shift ;;
        --code|--seed) PASSTHRU+=("$1" "$2"); shift 2 ;;
        -h|--help) sed -n '2,25p' "$0"; exit 0 ;;
        *) PASSTHRU+=("$1"); shift ;;
    esac
done

# 1. TypeScript submodule must be present so the verbose-run command works.
if [[ ! -d "$REPO_ROOT/TypeScript/tests" ]]; then
    echo "TypeScript submodule missing - initialising..." >&2
    git -C "$REPO_ROOT" submodule update --init TypeScript >&2
fi

# 2. Offline snapshot must be present so we can pick without re-running tsz.
if [[ ! -f "$REPO_ROOT/scripts/conformance/conformance-detail.json" ]]; then
    echo "conformance-detail.json missing — refresh the snapshot first:" >&2
    echo "  scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot" >&2
    exit 1
fi

# 3. Delegate the actual pick to the canonical implementation.
"$SCRIPT_DIR/pick.py" quick "${PASSTHRU[@]}" || exit $?

# 4. Optional source preview (one fewer step before reading the test).
if $SHOW && ! $RUN; then
    PATH_LINE="$("$SCRIPT_DIR/pick.py" one --filter ${PASSTHRU[@]+"${PASSTHRU[@]}"} 2>/dev/null || true)"
    if [[ -n "$PATH_LINE" ]]; then
        echo
        echo "==================== test source ===================="
        SOURCE_FILE="$REPO_ROOT/TypeScript/tests/cases/compiler/${PATH_LINE}.ts"
        # Conformance tests can also live under `cases/conformance/...`.
        if [[ ! -f "$SOURCE_FILE" ]]; then
            SOURCE_FILE="$(find "$REPO_ROOT/TypeScript/tests/cases" -name "${PATH_LINE}.ts" -print -quit 2>/dev/null || true)"
        fi
        if [[ -n "$SOURCE_FILE" && -f "$SOURCE_FILE" ]]; then
            head -80 "$SOURCE_FILE"
        else
            echo "(source file not found for: ${PATH_LINE})"
        fi
    fi
fi
