#!/usr/bin/env bash
# =============================================================================
# random-pick.sh — Quick random conformance failure picker
# =============================================================================
#
# A minimal, dependency-free random failure picker. Reads the offline snapshot
# at scripts/conformance/conformance-detail.json and prints one random failure.
#
# Usage:
#   scripts/session/random-pick.sh           # any random failure
#   scripts/session/random-pick.sh TS2322    # filter by error code
#
# Prints: path, category, expected/actual/missing/extra codes, pool size, and
# the verbose run command. Pure shell + python3 stdlib.
# =============================================================================
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
DETAIL="$REPO_ROOT/scripts/conformance/conformance-detail.json"

if [[ ! -f "$DETAIL" ]]; then
    echo "error: $DETAIL missing" >&2
    echo "  run: scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot" >&2
    exit 1
fi

if [[ ! -d "$REPO_ROOT/TypeScript/tests" ]]; then
    echo "TypeScript submodule not initialized — initializing..." >&2
    git -C "$REPO_ROOT" submodule update --init --depth 1 TypeScript
fi

CODE_FILTER="${1:-}"

python3 - "$DETAIL" "$CODE_FILTER" <<'PY'
import json, random, sys
from pathlib import Path

detail_path = sys.argv[1]
code_filter = sys.argv[2] or None

with open(detail_path, encoding="utf-8") as f:
    failures = json.load(f).get("failures", {})

def codes(entry):
    return set(entry.get("e", [])) | set(entry.get("a", [])) | \
           set(entry.get("m", [])) | set(entry.get("x", []))

def category(entry):
    e, a, m, x = entry.get("e", []), entry.get("a", []), entry.get("m", []), entry.get("x", [])
    if not e and a: return "false-positive"
    if e and not a: return "all-missing"
    if set(e) == set(a): return "fingerprint-only"
    if m and not x: return "only-missing"
    if x and not m: return "only-extra"
    return "wrong-code"

candidates = [(p, e) for p, e in failures.items()
              if e and (not code_filter or code_filter in codes(e))]
if not candidates:
    sys.exit(f"no matching failures (code={code_filter})")

path, entry = random.choice(candidates)
filter_name = Path(path).stem

print(f"path:     {path}")
print(f"category: {category(entry)}")
print(f"expected: {','.join(entry.get('e', [])) or '-'}")
print(f"actual:   {','.join(entry.get('a', [])) or '-'}")
print(f"missing:  {','.join(entry.get('m', [])) or '-'}")
print(f"extra:    {','.join(entry.get('x', [])) or '-'}")
print(f"pool:     {len(candidates)}")
print()
print(f'verbose run: ./scripts/conformance/conformance.sh run --filter "{filter_name}" --verbose')
PY
