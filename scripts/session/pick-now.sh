#!/usr/bin/env bash
# =============================================================================
# pick-now.sh — Tiny "give me a target right now" random failure picker
# =============================================================================
#
# A minimal-effort wrapper that:
#   1. Ensures the TypeScript submodule is initialised (so verbose runs work).
#   2. Picks one random failing conformance test from the offline snapshot.
#   3. Prints path, expected/actual codes, category, and a verbose-run command.
#
# Usage:
#   scripts/session/pick-now.sh                 # any failure
#   scripts/session/pick-now.sh --seed 7        # reproducible pick
#   scripts/session/pick-now.sh --code TS2322   # restrict to one error code
#
# Designed to be the absolute fastest path from "I want to start" to a
# concrete target. If you want more (source preview, source TS, ts run, etc.)
# use scripts/session/quick-pick.sh or scripts/session/pick-random.sh.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
DETAIL="$REPO_ROOT/scripts/conformance/conformance-detail.json"

SEED=""
CODE=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --seed) SEED="${2:-}"; shift 2 ;;
        --code) CODE="${2:-}"; shift 2 ;;
        -h|--help) sed -n '2,21p' "$0"; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
done

if [[ ! -f "$DETAIL" ]]; then
    echo "error: $DETAIL not found." >&2
    echo "  run: scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot" >&2
    exit 1
fi

if [[ ! -d "$REPO_ROOT/TypeScript/tests" ]]; then
    echo "TypeScript submodule missing — initialising..." >&2
    git -C "$REPO_ROOT" submodule update --init --depth 1 TypeScript >&2
fi

exec python3 - "$DETAIL" "$SEED" "$CODE" <<'PY'
import json, random, sys
from pathlib import Path

detail_path, seed, code = sys.argv[1:4]

with open(detail_path, encoding="utf-8") as f:
    failures = json.load(f).get("failures", {})

def classify(entry):
    e = entry.get("e", [])
    a = entry.get("a", [])
    m = entry.get("m", [])
    x = entry.get("x", [])
    if not e and a:
        return "false-positive"
    if e and not a:
        return "all-missing"
    if set(e) == set(a):
        return "fingerprint-only"
    if m and not x:
        return "only-missing"
    if x and not m:
        return "only-extra"
    return "wrong-code"

candidates = []
for path, entry in failures.items():
    if not entry:
        continue
    codes = (
        set(entry.get("e", []))
        | set(entry.get("a", []))
        | set(entry.get("m", []))
        | set(entry.get("x", []))
    )
    if code and code not in codes:
        continue
    candidates.append((path, entry))

if not candidates:
    sys.exit("no matching failures")

rng = random.Random(int(seed)) if seed else random.Random()
path, entry = rng.choice(candidates)
filter_name = Path(path).stem

def fmt(xs):
    return ",".join(xs) or "-"

print(f"path:     {path}")
print(f"category: {classify(entry)}")
print(f"expected: {fmt(entry.get('e', []))}")
print(f"actual:   {fmt(entry.get('a', []))}")
print(f"missing:  {fmt(entry.get('m', []))}")
print(f"extra:    {fmt(entry.get('x', []))}")
print(f"pool:     {len(candidates)}")
print()
print(
    f'verbose run: ./scripts/conformance/conformance.sh run --filter "{filter_name}" --verbose'
)
PY
