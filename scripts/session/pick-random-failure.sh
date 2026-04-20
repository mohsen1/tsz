#!/usr/bin/env bash
# =============================================================================
# pick-random-failure.sh — Canonical random conformance failure helper
# =============================================================================
#
# Usage:
#   scripts/session/pick-random-failure.sh                       # any category
#   scripts/session/pick-random-failure.sh fingerprint-only      # positional category
#   scripts/session/pick-random-failure.sh --fingerprint-only    # Tier 1 target
#   scripts/session/pick-random-failure.sh --wrong-code          # Tier 2 target
#   scripts/session/pick-random-failure.sh --code TS2322         # specific code
#   scripts/session/pick-random-failure.sh --one-extra           # leaf fix
#   scripts/session/pick-random-failure.sh --one-missing         # leaf fix
#   scripts/session/pick-random-failure.sh --show-source         # include test source
#   scripts/session/pick-random-failure.sh --show-tsc            # include tsc fingerprints
#   scripts/session/pick-random-failure.sh --source-lines 60     # source context size
#   scripts/session/pick-random-failure.sh --seed 42 --run       # reproducible + execute
#
# What it does:
#   1. Ensures the TypeScript submodule is initialized.
#   2. Picks a random failure from conformance-detail.json using the Python
#      picker, forwarding filter flags.
#   3. Optionally prints the test source and cached tsc fingerprints.
#   4. With --run, executes the target through the verbose conformance runner.
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
SHOW_SOURCE=false
SHOW_TSC=false
SOURCE_LINES=40
FORWARD_ARGS=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --run) RUN_AFTER=true; shift ;;
        --show-source) SHOW_SOURCE=true; shift ;;
        --show-tsc) SHOW_TSC=true; shift ;;
        --source-lines)
            SOURCE_LINES="$2"
            shift 2
            ;;
        --source-lines=*)
            SOURCE_LINES="${1#*=}"
            shift
            ;;
        # Category shortcuts — translate to the picker's --category flag.
        --fingerprint-only|--wrong-code|--only-missing|--only-extra|--all-missing|--false-positive)
            FORWARD_ARGS+=(--category "${1#--}"); shift ;;
        any|fingerprint-only|wrong-code|only-missing|only-extra|all-missing|false-positive)
            FORWARD_ARGS+=(--category "$1"); shift ;;
        -h|--help)
            sed -n '2,34p' "$0"
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

TARGET_PATH="$(echo "$PICK_OUTPUT" | awk '/^path: /{print $2; exit}')"
if [[ -z "$TARGET_PATH" ]]; then
    echo "error: could not parse picked test path" >&2
    exit 1
fi

if $SHOW_SOURCE; then
    ABS_PATH="$REPO_ROOT/$TARGET_PATH"
    echo
    echo -e "${CYAN}${BOLD}Source (${SOURCE_LINES} lines):${RESET} $TARGET_PATH"
    if [[ -f "$ABS_PATH" ]]; then
        head -n "$SOURCE_LINES" "$ABS_PATH"
        TOTAL_LINES="$(wc -l < "$ABS_PATH")"
        if (( TOTAL_LINES > SOURCE_LINES )); then
            echo "... ($((TOTAL_LINES - SOURCE_LINES)) more lines)"
        fi
    else
        echo "warn: source file not found at $ABS_PATH" >&2
    fi
fi

if $SHOW_TSC; then
    CACHE="$REPO_ROOT/scripts/conformance/tsc-cache-full.json"
    echo
    echo -e "${CYAN}${BOLD}tsc expected fingerprints:${RESET}"
    if [[ -f "$CACHE" ]]; then
        python3 - "$CACHE" "$TARGET_PATH" <<'PY'
import json, sys

cache_path, test_path = sys.argv[1], sys.argv[2]
with open(cache_path) as f:
    cache = json.load(f)

prefix = "TypeScript/tests/cases/"
key = test_path[len(prefix):] if test_path.startswith(prefix) else test_path
entry = cache.get(key) or cache.get(test_path) or {}
fps = entry.get("diagnostic_fingerprints") or []

if not fps:
    print("  (no tsc fingerprints recorded)")
else:
    for fp in fps:
        code = fp.get("code", "?")
        loc = f"{fp.get('file', '?')}:{fp.get('line', '?')}:{fp.get('column', '?')}"
        msg = fp.get("message_key") or fp.get("message") or ""
        print(f"  {code} {loc} - {msg}")
PY
    else
        echo "  (tsc cache missing at $CACHE)"
    fi
fi

if ! $RUN_AFTER; then
    echo
    echo -e "${CYAN}tip:${RESET} rerun with --run to execute it through conformance.sh --verbose"
    exit 0
fi

# conformance.sh --filter matches on the test name (basename without extension)
FILTER="$(basename "$TARGET_PATH")"
FILTER="${FILTER%.*}"

echo
echo -e "${CYAN}${BOLD}Running conformance with --verbose for: ${GREEN}$FILTER${RESET}"
exec "$REPO_ROOT/scripts/conformance/conformance.sh" run --filter "$FILTER" --verbose
