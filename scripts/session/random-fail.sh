#!/usr/bin/env bash
# =============================================================================
# random-fail.sh — Minimal random conformance failure picker
# =============================================================================
#
# Picks one random failing conformance test from the offline snapshot and
# prints it in an agent-friendly format. Auto-initialises the TypeScript
# submodule so the verbose-run command in the output is immediately usable.
#
# Usage:
#   scripts/session/random-fail.sh                 # any failure
#   scripts/session/random-fail.sh --seed 42       # reproducible pick
#   scripts/session/random-fail.sh --code TS2322   # filter by error code
#
# Output (always 6 lines + verbose command):
#   path:     <relative path>
#   category: <fingerprint-only|wrong-code|...>
#   expected: <comma-separated TS codes>
#   actual:   <comma-separated TS codes>
#   missing:  <codes tsc emits and we don't>
#   extra:    <codes we emit and tsc doesn't>
#
# Reads scripts/conformance/conformance-detail.json. Pure shell + python3.
# =============================================================================
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
DETAIL="$REPO_ROOT/scripts/conformance/conformance-detail.json"

SEED=""
CODE=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --seed) SEED="${2:?--seed requires a value}"; shift 2 ;;
        --code) CODE="${2:?--code requires a value}"; shift 2 ;;
        -h|--help) sed -n '2,21p' "$0"; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
done

if [[ ! -f "$DETAIL" ]]; then
    echo "error: $DETAIL missing" >&2
    echo "  run: scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot" >&2
    exit 1
fi

if [[ ! -d "$REPO_ROOT/TypeScript/tests" ]]; then
    echo "TypeScript submodule not initialised — initialising..." >&2
    git -C "$REPO_ROOT" submodule update --init --depth 1 TypeScript >&2
fi

SEED="$SEED" CODE="$CODE" DETAIL="$DETAIL" python3 - <<'PY'
import json, os, random, sys
from pathlib import Path

detail = Path(os.environ["DETAIL"])
seed = os.environ.get("SEED") or None
code = os.environ.get("CODE") or None

with detail.open(encoding="utf-8") as f:
    failures = json.load(f).get("failures", {})

def classify(e, a, m, x):
    if not e and a: return "false-positive"
    if e and not a: return "all-missing"
    if set(e) == set(a): return "fingerprint-only"
    if m and not x: return "only-missing"
    if x and not m: return "only-extra"
    return "wrong-code"

cands = []
for path, entry in failures.items():
    if not entry: continue
    e = list(entry.get("e", []))
    a = list(entry.get("a", []))
    m = list(entry.get("m", []))
    x = list(entry.get("x", []))
    if code and code not in (set(e) | set(a) | set(m) | set(x)):
        continue
    cands.append((path, e, a, m, x))

if not cands:
    sys.exit(f"no matching failures (code={code})")

rng = random.Random(int(seed)) if seed else random.Random()
path, e, a, m, x = rng.choice(cands)
fmt = lambda xs: ",".join(xs) or "-"
print(f"path:     {path}")
print(f"category: {classify(e, a, m, x)}")
print(f"expected: {fmt(e)}")
print(f"actual:   {fmt(a)}")
print(f"missing:  {fmt(m)}")
print(f"extra:    {fmt(x)}")
print(f"pool:     {len(cands)}")
print()
stem = Path(path).stem
print(f'verbose run: ./scripts/conformance/conformance.sh run --filter "{stem}" --verbose')
PY
