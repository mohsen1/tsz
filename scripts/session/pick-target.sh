#!/usr/bin/env bash
# =============================================================================
# pick-target.sh — Quick random failure picker for the conformance agent
# =============================================================================
#
# Tiny "give me a target" helper. Reads scripts/conformance/conformance-detail.json,
# picks one random failing conformance test, prints the path, expected/actual
# error codes, and a ready-to-paste verbose-run command.
#
# Differs from the other pickers by being deliberately minimal: no flags
# (other than --seed for reproducibility), no source preview, no auto-run.
# Use scripts/session/quick-pick.sh for the canonical agent workflow.
#
# Usage:
#   scripts/session/pick-target.sh           # pick a random failure
#   scripts/session/pick-target.sh --seed 7  # reproducible pick
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
DETAIL="$REPO_ROOT/scripts/conformance/conformance-detail.json"

SEED=""
case "${1:-}" in
    --seed) SEED="${2:?--seed requires a value}" ;;
    -h|--help) sed -n '2,17p' "$0"; exit 0 ;;
    "" ) ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
esac

if [[ ! -f "$DETAIL" ]]; then
    echo "error: $DETAIL not found." >&2
    echo "  run: scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot" >&2
    exit 1
fi

if [[ ! -d "$REPO_ROOT/TypeScript/tests" ]]; then
    echo "TypeScript submodule missing — initialising..." >&2
    git -C "$REPO_ROOT" submodule update --init --depth 1 TypeScript >&2
fi

exec python3 - "$DETAIL" "$SEED" <<'PY'
import json, random, sys
from pathlib import Path

detail_path, seed = sys.argv[1], sys.argv[2]
with open(detail_path, encoding="utf-8") as f:
    failures = json.load(f).get("failures", {})

candidates = [(p, e) for p, e in failures.items() if e]
if not candidates:
    sys.exit("no failures found in detail snapshot")

rng = random.Random(int(seed)) if seed else random.Random()
path, entry = rng.choice(candidates)
name = Path(path).stem

def fmt(xs): return ",".join(xs) or "-"

print(f"path:     {path}")
print(f"expected: {fmt(entry.get('e', []))}")
print(f"actual:   {fmt(entry.get('a', []))}")
print(f"missing:  {fmt(entry.get('m', []))}")
print(f"extra:    {fmt(entry.get('x', []))}")
print(f"pool:     {len(candidates)}")
print()
print(f'verbose run: ./scripts/conformance/conformance.sh run --filter "{name}" --verbose')
PY
