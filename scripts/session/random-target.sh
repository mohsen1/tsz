#!/usr/bin/env bash
# =============================================================================
# random-target.sh — Pick ONE random conformance failure to work on
# =============================================================================
#
# Quick, self-contained picker. Reads conformance-detail.json directly and
# prints a single random failure with everything you need to start working:
#
#   - path, category, expected/actual/missing/extra codes
#   - the verbose-run command to repro
#   - first 40 lines of the test source
#
# Usage:
#   scripts/session/random-target.sh                  # any failure
#   scripts/session/random-target.sh --code TS2322    # filter by code
#   scripts/session/random-target.sh --seed 42        # reproducible pick
#
# Notes:
#   - Ensures the TypeScript submodule is initialised so the verbose-run
#     command works against the test corpus.
#   - Refuses to roll if conformance-detail.json is missing — refresh with
#     `scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot`.
#   - This is the simplest possible picker; for advanced selection (categories,
#     close-to-passing, shortlists) use scripts/session/pick.py directly.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
DETAIL="$REPO_ROOT/scripts/conformance/conformance-detail.json"

CODE=""
SEED=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --code) CODE="$2"; shift 2 ;;
        --seed) SEED="$2"; shift 2 ;;
        -h|--help) sed -n '2,25p' "$0"; exit 0 ;;
        *) echo "unknown arg: $1" >&2; exit 2 ;;
    esac
done

# 1. TypeScript submodule must be checked out so the verbose-run command works.
if [[ ! -d "$REPO_ROOT/TypeScript/tests" ]]; then
    echo "TypeScript submodule missing — initialising..." >&2
    git -C "$REPO_ROOT" submodule update --init --depth 1 TypeScript >&2
fi

# 2. Snapshot data must exist (offline analysis, no full-suite rerun).
if [[ ! -f "$DETAIL" ]]; then
    echo "error: $DETAIL missing." >&2
    echo "  refresh with: scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot" >&2
    exit 1
fi

# 3. Pick one random failure inline. Uses python's random for reproducibility
#    when --seed is supplied. Filters to a specific error code if --code given.
PICK_JSON="$(python3 - "$DETAIL" "$CODE" "$SEED" <<'PY'
import json, random, sys
detail_path, code_filter, seed = sys.argv[1], sys.argv[2], sys.argv[3]
with open(detail_path, encoding="utf-8") as fh:
    failures = json.load(fh).get("failures", {})

candidates = []
for path, entry in failures.items():
    if not entry:
        continue
    codes = set(entry.get("e", []) or []) | set(entry.get("a", []) or []) \
          | set(entry.get("m", []) or []) | set(entry.get("x", []) or [])
    if code_filter and code_filter not in codes:
        continue
    candidates.append((path, entry))

if not candidates:
    print(json.dumps({"error": "no matching failures"}))
    sys.exit(0)

rng = random.Random(int(seed)) if seed else random.Random()
path, entry = rng.choice(candidates)

expected = entry.get("e", []) or []
actual   = entry.get("a", []) or []
missing  = entry.get("m", []) or []
extra    = entry.get("x", []) or []

if not expected and actual:
    category = "false-positive"
elif expected and not actual:
    category = "all-missing"
elif set(expected) == set(actual):
    category = "fingerprint-only"
elif missing and not extra:
    category = "only-missing"
elif extra and not missing:
    category = "only-extra"
else:
    category = "wrong-code"

print(json.dumps({
    "path": path,
    "category": category,
    "expected": expected,
    "actual": actual,
    "missing": missing,
    "extra": extra,
    "pool": len(candidates),
}))
PY
)"

if echo "$PICK_JSON" | python3 -c 'import json,sys; d=json.load(sys.stdin); sys.exit(0 if "error" not in d else 1)'; then
    :
else
    echo "$PICK_JSON" | python3 -c 'import json,sys; print(json.load(sys.stdin)["error"])' >&2
    exit 1
fi

PATH_VAL="$(echo "$PICK_JSON" | python3 -c 'import json,sys; print(json.load(sys.stdin)["path"])')"
CATEGORY="$(echo "$PICK_JSON" | python3 -c 'import json,sys; print(json.load(sys.stdin)["category"])')"
EXPECTED="$(echo "$PICK_JSON" | python3 -c 'import json,sys; print(",".join(json.load(sys.stdin)["expected"]) or "-")')"
ACTUAL="$(echo "$PICK_JSON"   | python3 -c 'import json,sys; print(",".join(json.load(sys.stdin)["actual"])   or "-")')"
MISSING="$(echo "$PICK_JSON"  | python3 -c 'import json,sys; print(",".join(json.load(sys.stdin)["missing"])  or "-")')"
EXTRA="$(echo "$PICK_JSON"    | python3 -c 'import json,sys; print(",".join(json.load(sys.stdin)["extra"])    or "-")')"
POOL="$(echo "$PICK_JSON"     | python3 -c 'import json,sys; print(json.load(sys.stdin)["pool"])')"
NAME="$(basename "$PATH_VAL" .tsx)"
NAME="$(basename "$NAME" .ts)"

echo "==================== random pick ===================="
echo "path:     $PATH_VAL"
echo "category: $CATEGORY"
echo "expected: $EXPECTED"
echo "actual:   $ACTUAL"
echo "missing:  $MISSING"
echo "extra:    $EXTRA"
echo "pool:     $POOL"
echo
echo "verbose run:"
echo "  ./scripts/conformance/conformance.sh run --filter \"$NAME\" --verbose"

# 4. Source preview — optional, fail-soft if file path is unusual.
SOURCE="$REPO_ROOT/$PATH_VAL"
if [[ -f "$SOURCE" ]]; then
    echo
    echo "==================== source (head) =================="
    head -40 "$SOURCE"
fi
