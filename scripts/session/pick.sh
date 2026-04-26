#!/usr/bin/env bash
# =============================================================================
# pick.sh — One-shot random conformance failure picker
# =============================================================================
#
# Picks a random failing conformance test, prints its codes/diff, the path,
# the first 40 lines of the source, and the verbose-run command. Designed for
# the "give me something to work on right now" workflow.
#
# Usage:
#   scripts/session/pick.sh                 # pick a random failure
#   scripts/session/pick.sh --code TS2322   # filter by error code
#   scripts/session/pick.sh --seed 42       # reproducible pick
#   scripts/session/pick.sh --run           # also run the verbose conformance
#
# Backed by the shared selector in scripts/session/pick.py so behavior matches
# quick-pick.sh; this wrapper adds a source preview before the verbose run.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
DETAIL="$REPO_ROOT/scripts/conformance/conformance-detail.json"

SEED=""
CODE=""
RUN=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --seed) SEED="${2:-}"; shift 2 ;;
        --code) CODE="${2:-}"; shift 2 ;;
        --run)  RUN=true; shift ;;
        -h|--help) sed -n '2,18p' "$0"; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
done

if [[ ! -d "$REPO_ROOT/TypeScript/tests" ]]; then
    echo "TypeScript submodule missing — initializing..." >&2
    git -C "$REPO_ROOT" submodule update --init --depth 1 TypeScript >&2
fi

if [[ ! -f "$DETAIL" ]]; then
    echo "error: $DETAIL not found." >&2
    echo "  run: scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot" >&2
    exit 1
fi

PICK_OUTPUT=$(REPO_ROOT="$REPO_ROOT" python3 - "$DETAIL" "$SEED" "$CODE" <<'PY'
import json, os, random, sys
from pathlib import Path

detail_path, seed, code = sys.argv[1], sys.argv[2], sys.argv[3]
repo_root = Path(os.environ["REPO_ROOT"])

with open(detail_path, encoding="utf-8") as f:
    failures = json.load(f).get("failures", {})

candidates = []
for path, entry in failures.items():
    if not entry:
        continue
    codes = (
        set(entry.get("e", [])) | set(entry.get("a", []))
        | set(entry.get("m", [])) | set(entry.get("x", []))
    )
    if code and code not in codes:
        continue
    candidates.append((path, entry))

if not candidates:
    sys.exit("no matching failures")

rng = random.Random(int(seed)) if seed else random.Random()
path, entry = rng.choice(candidates)
filter_name = Path(path).stem


def fmt(xs):
    return ",".join(xs) or "-"


print(f"path:     {path}")
print(f"expected: {fmt(entry.get('e', []))}")
print(f"actual:   {fmt(entry.get('a', []))}")
print(f"missing:  {fmt(entry.get('m', []))}")
print(f"extra:    {fmt(entry.get('x', []))}")
print(f"pool:     {len(candidates)}")
print(f"filter:   {filter_name}")

print()
print("------ source preview ------")
src = repo_root / path
if src.is_file():
    text = src.read_text(encoding="utf-8", errors="replace").splitlines()
    for line in text[:40]:
        print(line)
    if len(text) > 40:
        print(f"... ({len(text)} lines total)")
else:
    print(f"(source missing: {path})")

print()
print(f'verbose run: ./scripts/conformance/conformance.sh run --filter "{filter_name}" --verbose')
print(f"FILTER_NAME={filter_name}")
PY
)

echo "$PICK_OUTPUT"

if $RUN; then
    FILTER_NAME=$(printf '%s\n' "$PICK_OUTPUT" | sed -n 's/^FILTER_NAME=//p' | tail -1)
    if [[ -n "$FILTER_NAME" ]]; then
        echo
        echo "------ verbose run ------"
        exec "$REPO_ROOT/scripts/conformance/conformance.sh" run --filter "$FILTER_NAME" --verbose
    fi
fi
