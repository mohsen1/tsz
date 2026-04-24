#!/usr/bin/env bash
# =============================================================================
# pick-and-show.sh — Pick a random conformance failure and show everything
# =============================================================================
#
# One-stop script: picks a random failure from conformance-detail.json,
# prints the path/codes/category summary, shows the test source, and then
# runs the conformance runner with --verbose so the missing/extra
# fingerprints are printed in the same invocation.
#
# Usage:
#   scripts/session/pick-and-show.sh                # any failure
#   scripts/session/pick-and-show.sh --seed 42      # reproducible
#   scripts/session/pick-and-show.sh --code TS2322  # filter by error code
#
# See scripts/session/conformance-agent-prompt.md for the full process.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
DETAIL="$REPO_ROOT/scripts/conformance/conformance-detail.json"

SEED=""
CODE=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --seed) SEED="$2"; shift 2 ;;
        --code) CODE="$2"; shift 2 ;;
        -h|--help) sed -n '2,15p' "$0"; exit 0 ;;
        *) echo "unknown arg: $1" >&2; exit 2 ;;
    esac
done

if [[ ! -d "$REPO_ROOT/TypeScript/tests" ]]; then
    echo "TypeScript submodule missing — initializing..." >&2
    git -C "$REPO_ROOT" submodule update --init --depth 1 TypeScript >&2
fi
if [[ ! -f "$DETAIL" ]]; then
    echo "error: $DETAIL missing." >&2
    echo "  run: scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot" >&2
    exit 1
fi

PICK_JSON="$(SEED="$SEED" CODE="$CODE" python3 - "$DETAIL" <<'PY'
import json, os, random, sys

detail_path = sys.argv[1]
seed = os.environ.get("SEED") or None
code = os.environ.get("CODE") or None

with open(detail_path) as f:
    failures = json.load(f).get("failures", {})

def matches(entry):
    if not code:
        return True
    all_codes = set(entry.get("e", [])) | set(entry.get("a", [])) \
              | set(entry.get("m", [])) | set(entry.get("x", []))
    return code in all_codes

cands = [(p, e) for p, e in failures.items() if e and matches(e)]
if not cands:
    sys.exit("no matching failures")

rng = random.Random(int(seed)) if seed else random.Random()
path, entry = rng.choice(cands)

expected = entry.get("e", [])
actual   = entry.get("a", [])
missing  = entry.get("m", [])
extra    = entry.get("x", [])

if not expected and actual:        category = "false-positive"
elif expected and not actual:      category = "all-missing"
elif set(expected) == set(actual): category = "fingerprint-only"
elif missing and not extra:        category = "only-missing"
elif extra and not missing:        category = "only-extra"
else:                              category = "wrong-code"

print(json.dumps({
    "path": path,
    "filter": os.path.splitext(os.path.basename(path))[0],
    "category": category,
    "expected": expected,
    "actual": actual,
    "missing": missing,
    "extra": extra,
    "pool": len(cands),
}))
PY
)"

PATH_="$(printf '%s' "$PICK_JSON" | python3 -c 'import sys,json; print(json.load(sys.stdin)["path"])')"
FILTER="$(printf '%s' "$PICK_JSON" | python3 -c 'import sys,json; print(json.load(sys.stdin)["filter"])')"
CATEGORY="$(printf '%s' "$PICK_JSON" | python3 -c 'import sys,json; print(json.load(sys.stdin)["category"])')"

echo "==================== random pick ===================="
printf '%s' "$PICK_JSON" | python3 -c '
import sys, json
d = json.load(sys.stdin)
print(f"path:     {d[\"path\"]}")
print(f"category: {d[\"category\"]}")
print(f"expected: {(\",\".join(d[\"expected\"]) or \"-\")}")
print(f"actual:   {(\",\".join(d[\"actual\"]) or \"-\")}")
print(f"missing:  {(\",\".join(d[\"missing\"]) or \"-\")}")
print(f"extra:    {(\",\".join(d[\"extra\"]) or \"-\")}")
print(f"pool:     {d[\"pool\"]}")
'
echo ""
echo "==================== test source ===================="
if [[ -f "$REPO_ROOT/$PATH_" ]]; then
    sed -n '1,80p' "$REPO_ROOT/$PATH_"
    LINES=$(wc -l < "$REPO_ROOT/$PATH_")
    if [[ "$LINES" -gt 80 ]]; then
        echo "... (truncated at 80 lines; total $LINES)"
    fi
else
    echo "(source file missing: $PATH_)"
fi
echo ""
echo "==================== verbose run ===================="
exec "$REPO_ROOT/scripts/conformance/conformance.sh" run --filter "$FILTER" --verbose
