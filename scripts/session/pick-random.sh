#!/usr/bin/env bash
# =============================================================================
# pick-random.sh — Quick random failure picker with inline source preview
# =============================================================================
#
# Single-shot script for an agent that wants:
#   1. one random failing conformance test,
#   2. the expected/actual/missing/extra error codes,
#   3. the first ~40 lines of the offending TypeScript source,
#   4. a ready-to-paste verbose-run command.
#
# Usage:
#   scripts/session/pick-random.sh                 # any failure
#   scripts/session/pick-random.sh --seed 7        # reproducible pick
#   scripts/session/pick-random.sh --code TS2322   # restrict to one error code
#   scripts/session/pick-random.sh --no-source     # skip the source preview
#
# Reads scripts/conformance/conformance-detail.json. If the snapshot is
# missing, it tells you how to refresh it instead of running the full suite.
# Auto-initialises the TypeScript submodule if needed.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
DETAIL="$REPO_ROOT/scripts/conformance/conformance-detail.json"

SEED=""
CODE=""
SHOW_SOURCE=1

while [[ $# -gt 0 ]]; do
    case "$1" in
        --seed) SEED="${2:-}"; shift 2 ;;
        --code) CODE="${2:-}"; shift 2 ;;
        --no-source) SHOW_SOURCE=0; shift ;;
        -h|--help) sed -n '2,21p' "$0"; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
done

if [[ ! -f "$DETAIL" ]]; then
    echo "error: $DETAIL not found." >&2
    echo "  run: scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot" >&2
    exit 1
fi

if [[ ! -d "$REPO_ROOT/TypeScript/tests" ]]; then
    echo "TypeScript submodule missing — initialising..." >&2
    git -C "$REPO_ROOT" submodule update --init --depth 1 TypeScript >&2
fi

exec python3 - "$DETAIL" "$REPO_ROOT" "$SEED" "$CODE" "$SHOW_SOURCE" <<'PY'
import json, random, sys
from pathlib import Path

detail_path, repo_root, seed, code, show_source = sys.argv[1:6]
repo_root = Path(repo_root)

with open(detail_path, encoding="utf-8") as f:
    failures = json.load(f).get("failures", {})

candidates = []
for path, entry in failures.items():
    if not entry:
        continue
    codes = (
        set(entry.get("e", []))
        | set(entry.get("a", []))
        | set(entry.get("m", []))
        | set(entry.get("x", []))
    )
    if code and code not in codes:
        continue
    candidates.append((path, entry))

if not candidates:
    sys.exit("no matching failures")

rng = random.Random(int(seed)) if seed else random.Random()
path, entry = rng.choice(candidates)

def fmt(xs):
    return ",".join(xs) or "-"

filter_name = Path(path).stem
print(f"path:     {path}")
print(f"expected: {fmt(entry.get('e', []))}")
print(f"actual:   {fmt(entry.get('a', []))}")
print(f"missing:  {fmt(entry.get('m', []))}")
print(f"extra:    {fmt(entry.get('x', []))}")
print(f"pool:     {len(candidates)}")
print()
print(f'verbose run: ./scripts/conformance/conformance.sh run --filter "{filter_name}" --verbose')

if show_source == "1":
    src = repo_root / path
    if src.is_file():
        text = src.read_text(encoding="utf-8", errors="replace").splitlines()
        head = text[:40]
        print()
        print(f"--- source preview ({src}, first {len(head)}/{len(text)} lines) ---")
        for i, line in enumerate(head, 1):
            print(f"{i:>4}  {line}")
PY
