#!/usr/bin/env bash
# =============================================================================
# quick-pick.sh — Quickly pick ONE random conformance failure to work on.
# =============================================================================
#
# A minimal, dependency-light picker. Reads conformance-detail.json directly
# and prints a single random failing test plus its diagnostic diff and the
# command to run it under the conformance runner.
#
# Usage:
#   scripts/session/quick-pick.sh                  # any random failure
#   scripts/session/quick-pick.sh --code TS2322    # only failures touching TS2322
#   scripts/session/quick-pick.sh --seed 42        # reproducible pick
#   scripts/session/quick-pick.sh --category fingerprint-only
#
# Categories: fingerprint-only, wrong-code, only-missing, only-extra,
#             all-missing, false-positive, any (default).
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
DETAIL="$REPO_ROOT/scripts/conformance/conformance-detail.json"

CODE=""
SEED=""
CATEGORY="any"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --code)     CODE="$2"; shift 2 ;;
        --seed)     SEED="$2"; shift 2 ;;
        --category) CATEGORY="$2"; shift 2 ;;
        -h|--help)  sed -n '2,18p' "$0"; exit 0 ;;
        *) echo "unknown arg: $1" >&2; exit 2 ;;
    esac
done

# Ensure TypeScript submodule is checked out (idempotent, fast when already done).
if [[ ! -d "$REPO_ROOT/TypeScript/tests" ]]; then
    echo "TypeScript submodule missing — initializing..." >&2
    git -C "$REPO_ROOT" submodule update --init --depth 1 TypeScript >&2
fi

if [[ ! -f "$DETAIL" ]]; then
    echo "error: $DETAIL not found." >&2
    echo "  Run: scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot" >&2
    exit 1
fi

python3 - "$DETAIL" "$CODE" "$SEED" "$CATEGORY" <<'PY'
import json, os, random, sys

detail_path, code, seed, category = sys.argv[1:5]

with open(detail_path) as f:
    failures = json.load(f).get("failures", {})

def classify(entry):
    e, a = set(entry.get("e", [])), set(entry.get("a", []))
    m, x = set(entry.get("m", [])), set(entry.get("x", []))
    if not e and a:        return "false-positive"
    if e and not a:        return "all-missing"
    if e == a:             return "fingerprint-only"
    if m and not x:        return "only-missing"
    if x and not m:        return "only-extra"
    return "wrong-code"

candidates = []
for path, entry in failures.items():
    if not entry:
        continue
    cat = classify(entry)
    if category != "any" and cat != category:
        continue
    if code:
        all_codes = (set(entry.get("e", [])) | set(entry.get("a", []))
                     | set(entry.get("m", [])) | set(entry.get("x", [])))
        if code not in all_codes:
            continue
    candidates.append((path, entry, cat))

if not candidates:
    sys.exit("no failures match the requested filters")

rng = random.Random(int(seed)) if seed else random.Random()
path, entry, cat = rng.choice(candidates)
basename = os.path.splitext(os.path.basename(path))[0]

def fmt(xs): return ",".join(xs) if xs else "-"

print(f"path:     {path}")
print(f"category: {cat}  (pool: {len(candidates)})")
print(f"expected: {fmt(entry.get('e', []))}")
print(f"actual:   {fmt(entry.get('a', []))}")
print(f"missing:  {fmt(entry.get('m', []))}")
print(f"extra:    {fmt(entry.get('x', []))}")
print()
print(f"verbose run: ./scripts/conformance/conformance.sh run --filter \"{basename}\" --verbose")
PY
