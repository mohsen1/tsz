#!/usr/bin/env bash
# random-failure.sh — minimal one-shot random conformance failure picker.
#
# Differs from quick-pick.sh by being a small wrapper that prints a single
# JSON-ish line and the verbose-run command. Use either; this one is tuned
# for cheap shell consumption.
#
# Usage:
#   scripts/session/random-failure.sh           # any failure
#   scripts/session/random-failure.sh TS2322    # filter to a specific code
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
DETAIL="$REPO_ROOT/scripts/conformance/conformance-detail.json"
CODE="${1:-}"

if [[ ! -f "$DETAIL" ]]; then
    echo "error: $DETAIL missing — run scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot" >&2
    exit 1
fi

CODE="$CODE" python3 - "$DETAIL" <<'PY'
import json, os, random, sys
detail_path = sys.argv[1]
code = os.environ.get("CODE") or None
with open(detail_path) as f:
    failures = json.load(f).get("failures", {})

def matches(entry):
    if not code:
        return True
    pool = set(entry.get("e", [])) | set(entry.get("a", []))
    pool |= set(entry.get("m", [])) | set(entry.get("x", []))
    return code in pool

cands = [(p, e) for p, e in failures.items() if e and matches(e)]
if not cands:
    sys.exit("no matching failures")

path, entry = random.choice(cands)
filt = os.path.splitext(os.path.basename(path))[0]
print(f"path={path}")
print(f"expected={','.join(entry.get('e', [])) or '-'}")
print(f"actual={','.join(entry.get('a', [])) or '-'}")
print(f"missing={','.join(entry.get('m', [])) or '-'}")
print(f"extra={','.join(entry.get('x', [])) or '-'}")
print(f"verbose-run: ./scripts/conformance/conformance.sh run --filter \"{filt}\" --verbose")
PY
