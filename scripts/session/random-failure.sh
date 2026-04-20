#!/usr/bin/env bash
# =============================================================================
# random-failure.sh — Pick ONE random conformance failure, tier-weighted.
# =============================================================================
#
# A "quick start" picker for conformance agents. Unlike pick-random-failure.sh,
# this draws one failure using the campaign tier weighting from the session
# protocol and prints a ready-to-paste verbose-run command.
#
# Usage:
#   scripts/session/random-failure.sh              # one random target
#   scripts/session/random-failure.sh --run        # also run the verbose diff
#   scripts/session/random-failure.sh --seed 42    # reproducible pick
#   scripts/session/random-failure.sh --tier 1     # force a specific tier
#
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
DETAIL="$REPO_ROOT/scripts/conformance/conformance-detail.json"
PICKER="$SCRIPT_DIR/pick-random-failure.py"

if [[ -t 1 ]]; then
    CYAN='\033[0;36m' GREEN='\033[0;32m' YELLOW='\033[0;33m' BOLD='\033[1m' RESET='\033[0m'
else
    CYAN='' GREEN='' YELLOW='' BOLD='' RESET=''
fi

RUN_AFTER=false
SEED=""
FORCE_TIER=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --run) RUN_AFTER=true; shift ;;
        --seed) SEED="$2"; shift 2 ;;
        --tier) FORCE_TIER="$2"; shift 2 ;;
        -h|--help) sed -n '2,22p' "$0"; exit 0 ;;
        *) echo "unknown arg: $1" >&2; exit 2 ;;
    esac
done

# --- 1. TypeScript submodule ---
if [[ ! -d "$REPO_ROOT/TypeScript/tests" ]]; then
    echo -e "${YELLOW}!${RESET} TypeScript submodule not initialized — running submodule update…"
    git -C "$REPO_ROOT" submodule update --init --depth 1 TypeScript
fi

# --- 2. Conformance snapshot ---
if [[ ! -f "$DETAIL" ]]; then
    echo -e "${YELLOW}!${RESET} $DETAIL missing." >&2
    echo "  Run: scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot" >&2
    exit 1
fi

# --- 3. Draw one tier-weighted failure ---
PICK_ARGS=(--weighted-tier --count 1 --json)
if [[ -n "$SEED" ]]; then
    PICK_ARGS+=(--seed "$SEED")
fi
if [[ -n "$FORCE_TIER" ]]; then
    PICK_ARGS+=(--tier "$FORCE_TIER")
fi

PICK="$(python3 "$PICKER" "${PICK_ARGS[@]}")"

# --- 4. Pretty-print and emit the run command ---
python3 - "$PICK" "$REPO_ROOT" <<'PY'
import json, os, sys

pick = json.loads(sys.argv[1])
repo = sys.argv[2]

def fmt(xs): return ",".join(xs) if xs else "-"

basename = os.path.splitext(os.path.basename(pick["path"]))[0]

print(f"\033[1mtier:\033[0m      {pick['tier']}  (pool size: {pick['pool_size']})")
print(f"\033[1mcategory:\033[0m  {pick['category']}")
print(f"\033[1mpath:\033[0m      {pick['path']}")
print(f"\033[1mexpected:\033[0m  {fmt(pick['expected'])}")
print(f"\033[1mactual:\033[0m    {fmt(pick['actual'])}")
print(f"\033[1mmissing:\033[0m   {fmt(pick['missing'])}")
print(f"\033[1mextra:\033[0m     {fmt(pick['extra'])}")
print(f"\033[1mdiff:\033[0m      {pick['diff']}")
print()
print(f"\033[36mverbose run:\033[0m ./scripts/conformance/conformance.sh run --filter \"{basename}\" --verbose")
PY

if ! $RUN_AFTER; then
    echo
    echo -e "${CYAN}tip:${RESET} rerun with --run to execute the verbose diff immediately"
    exit 0
fi

FILTER="$(python3 -c 'import json,sys,os; p=json.loads(sys.argv[1])["path"]; print(os.path.splitext(os.path.basename(p))[0])' "$PICK")"
echo
echo -e "${CYAN}${BOLD}Running conformance with --verbose for: ${GREEN}$FILTER${RESET}"
exec "$REPO_ROOT/scripts/conformance/conformance.sh" run --filter "$FILTER" --verbose
