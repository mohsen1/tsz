#!/usr/bin/env bash
# =============================================================================
# roll-failure.sh — Quick pick of a random conformance failure
# =============================================================================
#
# Rolls one random failing test from the latest conformance snapshot,
# preferring Tier 1 fingerprint-only targets by default, and prints the
# ready-to-use `conformance.sh --filter` command.
#
# Usage:
#   scripts/session/roll-failure.sh                 # fingerprint-only (default)
#   scripts/session/roll-failure.sh any             # any category
#   scripts/session/roll-failure.sh wrong-code
#   scripts/session/roll-failure.sh --code TS2322
#   scripts/session/roll-failure.sh --seed 42
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
DETAIL="$REPO_ROOT/scripts/conformance/conformance-detail.json"

[[ -d "$REPO_ROOT/TypeScript/tests" ]] || \
    git -C "$REPO_ROOT" submodule update --init --depth 1 TypeScript >&2

if [[ ! -f "$DETAIL" ]]; then
    echo "error: $DETAIL missing; run scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot" >&2
    exit 1
fi

CATEGORY="fingerprint-only"
ARGS=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        any|fingerprint-only|wrong-code|only-missing|only-extra|all-missing|false-positive)
            CATEGORY="$1"; shift ;;
        *) ARGS+=("$1"); shift ;;
    esac
done

exec python3 - "$DETAIL" "$CATEGORY" "${ARGS[@]}" <<'PY'
import json, random, sys, argparse, os

detail_path, category, *rest = sys.argv[1:]
ap = argparse.ArgumentParser()
ap.add_argument("--code")
ap.add_argument("--seed", type=int)
args = ap.parse_args(rest)

def classify(e):
    exp, act = set(e.get("e", [])), set(e.get("a", []))
    miss, extra = set(e.get("m", [])), set(e.get("x", []))
    if not exp and act: return "false-positive"
    if exp and not act: return "all-missing"
    if exp == act:      return "fingerprint-only"
    if miss and not extra: return "only-missing"
    if extra and not miss: return "only-extra"
    return "wrong-code"

with open(detail_path) as f:
    failures = json.load(f).get("failures", {})

pool = []
for path, entry in failures.items():
    if not entry: continue
    if category != "any" and classify(entry) != category: continue
    if args.code:
        codes = set(entry.get("e", [])) | set(entry.get("a", [])) \
              | set(entry.get("m", [])) | set(entry.get("x", []))
        if args.code not in codes: continue
    pool.append((path, entry))

if not pool:
    sys.exit(f"no failures match category={category} code={args.code}")

rng = random.Random(args.seed)
path, entry = rng.choice(pool)
cat = classify(entry)
filt = os.path.splitext(os.path.basename(path))[0]

print(f"path:     {path}")
print(f"category: {cat}")
print(f"expected: {','.join(entry.get('e', [])) or '-'}")
print(f"actual:   {','.join(entry.get('a', [])) or '-'}")
print(f"missing:  {','.join(entry.get('m', [])) or '-'}")
print(f"extra:    {','.join(entry.get('x', [])) or '-'}")
print(f"diff:     {len(entry.get('m', [])) + len(entry.get('x', []))}")
print()
print(f"pool:     {len(pool)} candidates")
print(f"run:      ./scripts/conformance/conformance.sh run --filter '{filt}' --verbose")
PY
