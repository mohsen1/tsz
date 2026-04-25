#!/usr/bin/env bash
# =============================================================================
# random-failure.sh — Tiny, self-contained random conformance-failure picker
# =============================================================================
#
# A minimal "give me one failure to work on" wrapper that does NOT depend on
# pick.py. Reads scripts/conformance/conformance-detail.json directly, picks
# one random failing test, and prints the path, codes, and a verbose-run
# command.
#
# Usage:
#   scripts/session/random-failure.sh              # any failure
#   scripts/session/random-failure.sh --seed 42    # reproducible
#   scripts/session/random-failure.sh --code TS2322 # filter by error code
#
# This is intentionally separate from quick-pick.sh — it is a single-file,
# zero-config alternative for agents that just want a random target fast.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
DETAIL="$REPO_ROOT/scripts/conformance/conformance-detail.json"

SEED=""
CODE=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --seed) SEED="${2:-}"; shift 2 ;;
        --code) CODE="${2:-}"; shift 2 ;;
        -h|--help)
            sed -n '2,18p' "$0"
            exit 0
            ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
done

if [[ ! -f "$DETAIL" ]]; then
    echo "error: $DETAIL not found." >&2
    echo "  run: scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot" >&2
    exit 1
fi

if [[ ! -d "$REPO_ROOT/TypeScript/tests" ]]; then
    echo "TypeScript submodule missing — initializing..." >&2
    git -C "$REPO_ROOT" submodule update --init --depth 1 TypeScript >&2
fi

exec python3 - "$DETAIL" "$SEED" "$CODE" <<'PY'
import json, random, sys
from pathlib import Path

detail_path, seed, code = sys.argv[1], sys.argv[2], sys.argv[3]
with open(detail_path, encoding="utf-8") as f:
    failures = json.load(f).get("failures", {})

candidates = []
for path, entry in failures.items():
    if not entry:
        continue
    codes = set(entry.get("e", [])) | set(entry.get("a", [])) \
          | set(entry.get("m", [])) | set(entry.get("x", []))
    if code and code not in codes:
        continue
    candidates.append((path, entry))

if not candidates:
    sys.exit("no matching failures")

rng = random.Random(int(seed)) if seed else random.Random()
path, entry = rng.choice(candidates)
filter_name = Path(path).stem

def fmt(xs): return ",".join(xs) or "-"

print(f"path:     {path}")
print(f"expected: {fmt(entry.get('e', []))}")
print(f"actual:   {fmt(entry.get('a', []))}")
print(f"missing:  {fmt(entry.get('m', []))}")
print(f"extra:    {fmt(entry.get('x', []))}")
print(f"pool:     {len(candidates)}")
print()
print(f'verbose run: ./scripts/conformance/conformance.sh run --filter "{filter_name}" --verbose')
PY
