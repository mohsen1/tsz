#!/usr/bin/env bash
# =============================================================================
# pick-random.sh — One-liner random conformance failure picker
# =============================================================================
#
# A minimal, no-frills companion to quick-pick.sh. Prints ONE random failing
# test name per line (no metadata) from conformance-detail.json — ideal for
# piping into xargs or quick shell loops.
#
# Usage:
#   scripts/session/pick-random.sh                 # one random filter name
#   scripts/session/pick-random.sh 5               # five random filter names
#   scripts/session/pick-random.sh --code TS2322   # filter by error code
#   scripts/session/pick-random.sh --category fingerprint-only
#   scripts/session/pick-random.sh --seed 42
#
# For the full metadata-printing picker, use scripts/session/quick-pick.sh.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
DETAIL="$REPO_ROOT/scripts/conformance/conformance-detail.json"

N=1
CODE=""
CATEGORY=""
SEED=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --code) CODE="$2"; shift 2 ;;
        --category) CATEGORY="$2"; shift 2 ;;
        --seed) SEED="$2"; shift 2 ;;
        -h|--help) sed -n '2,18p' "$0"; exit 0 ;;
        [0-9]*) N="$1"; shift ;;
        *) echo "unknown arg: $1" >&2; exit 2 ;;
    esac
done

if [[ ! -f "$DETAIL" ]]; then
    echo "error: $DETAIL missing — run 'scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot'" >&2
    exit 1
fi

N="$N" CODE="$CODE" CATEGORY="$CATEGORY" SEED="$SEED" python3 - "$DETAIL" <<'PY'
import json, os, random, sys

detail_path = sys.argv[1]
n = int(os.environ.get("N") or "1")
code = os.environ.get("CODE") or None
category = os.environ.get("CATEGORY") or None
seed = os.environ.get("SEED") or None

with open(detail_path) as f:
    failures = json.load(f).get("failures", {})

def classify(entry):
    e, a = entry.get("e", []), entry.get("a", [])
    m, x = entry.get("m", []), entry.get("x", [])
    if not e and a: return "false-positive"
    if e and not a: return "all-missing"
    if set(e) == set(a): return "fingerprint-only"
    if m and not x: return "only-missing"
    if x and not m: return "only-extra"
    return "wrong-code"

def matches(entry):
    if code:
        all_codes = set(entry.get("e", [])) | set(entry.get("a", [])) \
                  | set(entry.get("m", [])) | set(entry.get("x", []))
        if code not in all_codes:
            return False
    if category and classify(entry) != category:
        return False
    return True

cands = [p for p, e in failures.items() if e and matches(e)]
if not cands:
    sys.exit("no matching failures")

rng = random.Random(int(seed)) if seed else random.Random()
rng.shuffle(cands)
for p in cands[:n]:
    print(os.path.splitext(os.path.basename(p))[0])
PY
