#!/usr/bin/env bash
# =============================================================================
# shortlist-failures.sh — Print N random conformance failures for quick triage.
# =============================================================================
#
# Wraps quick-pick.sh with a small helper that surfaces N random failures at
# once. The first failure printed is what you commit to working on; the rest
# are for context (so you can see category diversity without re-running the
# full suite).
#
# Usage:
#   scripts/session/shortlist-failures.sh            # 5 picks
#   scripts/session/shortlist-failures.sh 3          # 3 picks
#   scripts/session/shortlist-failures.sh 5 TS2322   # 5 picks filtered to TS2322
#
# Do NOT reroll to avoid hard targets — per conformance-agent-prompt.md the
# first target is yours. This script is only a survey tool.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
DETAIL="$REPO_ROOT/scripts/conformance/conformance-detail.json"

COUNT="${1:-5}"
CODE="${2:-}"

if [[ ! -f "$DETAIL" ]]; then
    echo "error: $DETAIL missing." >&2
    echo "  run: scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot" >&2
    exit 1
fi

COUNT="$COUNT" CODE="$CODE" python3 - "$DETAIL" <<'PY'
import json, os, random, sys

detail_path = sys.argv[1]
count = int(os.environ.get("COUNT") or "5")
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

rng = random.Random()
picks = rng.sample(cands, min(count, len(cands)))

for i, (path, entry) in enumerate(picks, 1):
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

    print(f"[{i}] {path}")
    print(f"     category: {category}")
    print(f"     expected: {','.join(expected) or '-'}")
    print(f"     actual:   {','.join(actual)   or '-'}")
    print(f"     missing:  {','.join(missing)  or '-'}")
    print(f"     extra:    {','.join(extra)    or '-'}")
    print(f"     verbose:  ./scripts/conformance/conformance.sh run --filter \"{filt}\" --verbose")
    print()
PY
