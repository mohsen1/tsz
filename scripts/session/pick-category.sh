#!/usr/bin/env bash
# =============================================================================
# pick-category.sh — Random failure picker filtered by category
# =============================================================================
#
# Picks ONE random failure from conformance-detail.json, optionally filtered
# by failure category and/or error code. Prints the essentials and the
# verbose-run command.
#
# Categories:
#   any              — any failure (default)
#   fingerprint-only — same codes, wrong position/message/count
#   false-positive   — tsc expects 0, we emit errors
#   all-missing      — we emit 0, tsc expects errors
#   one-extra        — one extra code, 0 missing
#   one-missing      — one missing code, 0 extra
#   wrong-code       — both sides have errors, codes differ
#   close            — diff <= N (use --diff to set N, default 2)
#
# Usage:
#   scripts/session/pick-category.sh --category one-extra
#   scripts/session/pick-category.sh --category fingerprint-only --code TS2322
#   scripts/session/pick-category.sh --category close --diff 2 --seed 7
#   scripts/session/pick-category.sh --category false-positive --run
#
# See scripts/session/conformance-agent-prompt.md for the full workflow.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
DETAIL="$REPO_ROOT/scripts/conformance/conformance-detail.json"

CATEGORY="any"
SEED=""
CODE=""
DIFF=2
RUN_AFTER=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --category) CATEGORY="$2"; shift 2 ;;
        --seed) SEED="$2"; shift 2 ;;
        --code) CODE="$2"; shift 2 ;;
        --diff) DIFF="$2"; shift 2 ;;
        --run) RUN_AFTER=true; shift ;;
        -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
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

FILTER="$(CATEGORY="$CATEGORY" SEED="$SEED" CODE="$CODE" DIFF="$DIFF" \
    python3 - "$DETAIL" <<'PY'
import json, os, random, sys

detail_path = sys.argv[1]
category = os.environ.get("CATEGORY", "any")
seed = os.environ.get("SEED") or None
code = os.environ.get("CODE") or None
diff_limit = int(os.environ.get("DIFF", "2"))

with open(detail_path) as f:
    failures = json.load(f).get("failures", {})

def classify(entry):
    expected = entry.get("e", [])
    actual   = entry.get("a", [])
    missing  = entry.get("m", [])
    extra    = entry.get("x", [])
    if not expected and actual:        return "false-positive"
    if expected and not actual:        return "all-missing"
    if set(expected) == set(actual):   return "fingerprint-only"
    if missing and not extra:          return "only-missing"
    if extra and not missing:          return "only-extra"
    return "wrong-code"

def matches(entry):
    missing = entry.get("m", [])
    extra   = entry.get("x", [])
    cat     = classify(entry)
    if code:
        all_codes = (set(entry.get("e", [])) | set(entry.get("a", []))
                   | set(missing) | set(extra))
        if code not in all_codes:
            return False
    if category == "any":
        return True
    if category == "fingerprint-only":
        return cat == "fingerprint-only"
    if category == "false-positive":
        return cat == "false-positive"
    if category == "all-missing":
        return cat == "all-missing"
    if category == "wrong-code":
        return cat == "wrong-code"
    if category == "one-extra":
        return not missing and len(extra) == 1
    if category == "one-missing":
        return not extra and len(missing) == 1
    if category == "close":
        return (len(missing) + len(extra)) <= diff_limit and (missing or extra)
    sys.exit(f"unknown category: {category}")

cands = [(p, e) for p, e in failures.items() if e and matches(e)]
if not cands:
    sys.exit(f"no failures matching category={category} code={code}")

rng = random.Random(int(seed)) if seed else random.Random()
path, entry = rng.choice(cands)
filt = os.path.splitext(os.path.basename(path))[0]
cat = classify(entry)

print(f"category: {category} (resolved: {cat})",         file=sys.stderr)
print(f"path:     {path}",                               file=sys.stderr)
print(f"expected: {','.join(entry.get('e', [])) or '-'}", file=sys.stderr)
print(f"actual:   {','.join(entry.get('a', [])) or '-'}", file=sys.stderr)
print(f"missing:  {','.join(entry.get('m', [])) or '-'}", file=sys.stderr)
print(f"extra:    {','.join(entry.get('x', [])) or '-'}", file=sys.stderr)
print(f"pool:     {len(cands)}",                         file=sys.stderr)
print("",                                                file=sys.stderr)
print(f"verbose run: ./scripts/conformance/conformance.sh run --filter \"{filt}\" --verbose",
      file=sys.stderr)
print(filt)
PY
)"

if $RUN_AFTER; then
    echo
    echo "Running: ./scripts/conformance/conformance.sh run --filter \"$FILTER\" --verbose"
    exec "$REPO_ROOT/scripts/conformance/conformance.sh" run --filter "$FILTER" --verbose
fi
