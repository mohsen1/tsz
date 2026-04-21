#!/usr/bin/env bash
# =============================================================================
# quick-pick.sh — Minimal random failure picker
# =============================================================================
#
# A tiny "just give me something to work on" wrapper. Picks one random
# conformance failure from conformance-detail.json, prints the essentials,
# and shows the command to run it verbosely.
#
# Usage:
#   scripts/session/quick-pick.sh              # any failure
#   scripts/session/quick-pick.sh --seed 42    # reproducible
#   scripts/session/quick-pick.sh --code TS2322  # filter by error code
#   scripts/session/quick-pick.sh --run        # also run conformance --verbose
#
# This is the sole random-failure picker for the conformance agent workflow.
# See scripts/session/conformance-agent-prompt.md for the full process.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
DETAIL="$REPO_ROOT/scripts/conformance/conformance-detail.json"

SEED=""
CODE=""
RUN_AFTER=false
while [[ $# -gt 0 ]]; do
    case "$1" in
        --seed) SEED="$2"; shift 2 ;;
        --code) CODE="$2"; shift 2 ;;
        --run) RUN_AFTER=true; shift ;;
        -h|--help) sed -n '2,17p' "$0"; exit 0 ;;
        *) echo "unknown arg: $1" >&2; exit 2 ;;
    esac
done

# Ensure submodule and snapshot exist.
if [[ ! -d "$REPO_ROOT/TypeScript/tests" ]]; then
    echo "TypeScript submodule missing — initializing..." >&2
    git -C "$REPO_ROOT" submodule update --init --depth 1 TypeScript >&2
fi
if [[ ! -f "$DETAIL" ]]; then
    echo "error: $DETAIL missing." >&2
    echo "  run: scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot" >&2
    exit 1
fi

FILTER="$(SEED="$SEED" CODE="$CODE" python3 - "$DETAIL" <<'PY'
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

filt = os.path.splitext(os.path.basename(path))[0]

# Human-readable summary → stderr. Filter name → stdout (captured by caller).
print(f"path:     {path}",                    file=sys.stderr)
print(f"category: {category}",                file=sys.stderr)
print(f"expected: {','.join(expected) or '-'}", file=sys.stderr)
print(f"actual:   {','.join(actual)   or '-'}", file=sys.stderr)
print(f"missing:  {','.join(missing)  or '-'}", file=sys.stderr)
print(f"extra:    {','.join(extra)    or '-'}", file=sys.stderr)
print(f"pool:     {len(cands)}",              file=sys.stderr)
print("",                                     file=sys.stderr)
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
