#!/usr/bin/env bash
# spin.sh — pick one random conformance failure to work on.
#
# Reads scripts/conformance/conformance-detail.json and prints a single
# random failing test with its expected/actual/missing/extra error codes
# plus the verbose conformance run command.
#
# Usage:
#   scripts/session/spin.sh                  # any random failure
#   scripts/session/spin.sh --seed 42        # reproducible pick
#   scripts/session/spin.sh --code TS2322    # restrict to one error family
#   scripts/session/spin.sh --category fingerprint-only
#   scripts/session/spin.sh --run            # also exec the verbose runner
#
# Categories: fingerprint-only, false-positive, all-missing, only-missing,
#             only-extra, wrong-code.
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
DETAIL="$REPO_ROOT/scripts/conformance/conformance-detail.json"

if [[ ! -d "$REPO_ROOT/TypeScript/tests" ]]; then
    echo "TypeScript submodule missing — initializing..." >&2
    git -C "$REPO_ROOT" submodule update --init --depth 1 TypeScript >&2
fi

if [[ ! -f "$DETAIL" ]]; then
    echo "error: $DETAIL not found." >&2
    echo "  run: scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot" >&2
    exit 1
fi

SEED=""
CODE=""
CATEGORY=""
RUN=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --seed)     SEED="$2"; shift 2 ;;
        --code)     CODE="$2"; shift 2 ;;
        --category) CATEGORY="$2"; shift 2 ;;
        --run)      RUN=1; shift ;;
        -h|--help)  sed -n '2,15p' "$0"; exit 0 ;;
        *)          echo "unknown arg: $1" >&2; exit 2 ;;
    esac
done

PICK_OUTPUT="$(SEED="$SEED" CODE="$CODE" CATEGORY="$CATEGORY" DETAIL="$DETAIL" python3 - <<'PY'
import json, os, random, sys
from pathlib import Path

detail_path = os.environ["DETAIL"]
seed = os.environ.get("SEED") or None
code = os.environ.get("CODE") or None
category_filter = os.environ.get("CATEGORY") or None

with open(detail_path, encoding="utf-8") as f:
    failures = json.load(f).get("failures", {})

def codes_of(entry):
    return (set(entry.get("e", [])) | set(entry.get("a", []))
            | set(entry.get("m", [])) | set(entry.get("x", [])))

def category(entry):
    e, a, m, x = (entry.get("e", []), entry.get("a", []),
                  entry.get("m", []), entry.get("x", []))
    if not e and a: return "false-positive"
    if e and not a: return "all-missing"
    if set(e) == set(a): return "fingerprint-only"
    if m and not x: return "only-missing"
    if x and not m: return "only-extra"
    return "wrong-code"

pool = []
for path, entry in failures.items():
    if not entry:
        continue
    if code and code not in codes_of(entry):
        continue
    if category_filter and category(entry) != category_filter:
        continue
    pool.append((path, entry))

if not pool:
    sys.exit(f"no matching failures (code={code} category={category_filter})")

rng = random.Random(int(seed)) if seed else random.Random()
path, entry = rng.choice(pool)
name = Path(path).stem
fmt = lambda xs: ",".join(xs) or "-"
print(f"path:     {path}")
print(f"category: {category(entry)}")
print(f"expected: {fmt(entry.get('e', []))}")
print(f"actual:   {fmt(entry.get('a', []))}")
print(f"missing:  {fmt(entry.get('m', []))}")
print(f"extra:    {fmt(entry.get('x', []))}")
print(f"pool:     {len(pool)}")
print()
print(f'verbose run: ./scripts/conformance/conformance.sh run --filter "{name}" --verbose')
print(f"__FILTER__={name}")
PY
)"

echo "$PICK_OUTPUT" | grep -v '^__FILTER__='

if [[ "$RUN" -eq 1 ]]; then
    FILTER="$(echo "$PICK_OUTPUT" | sed -n 's/^__FILTER__=//p')"
    exec "$REPO_ROOT/scripts/conformance/conformance.sh" run --filter "$FILTER" --verbose
fi
