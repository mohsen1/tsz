#!/usr/bin/env bash
# =============================================================================
# roll.sh — One-line "roll the dice" random conformance failure picker
# =============================================================================
#
# The most compact picker in scripts/session. Prints exactly one line:
#
#   <category>  <filter-name>  <expected>  <actual>  <missing>  <extra>
#
# Plus, when stdout is a TTY, a second line with the verbose-run command.
# Designed for grep/awk pipelines and quick "what should I work on" prompts.
#
# Usage:
#   scripts/session/roll.sh
#   scripts/session/roll.sh --code TS2322
#   scripts/session/roll.sh --category fingerprint-only
#   scripts/session/roll.sh --seed 13
#
# Reads scripts/conformance/conformance-detail.json. Auto-initialises the
# TypeScript submodule when missing so the verbose run command actually works.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
DETAIL="$REPO_ROOT/scripts/conformance/conformance-detail.json"

SEED=""
CODE=""
CATEGORY=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --seed) SEED="${2:-}"; shift 2 ;;
        --code) CODE="${2:-}"; shift 2 ;;
        --category) CATEGORY="${2:-}"; shift 2 ;;
        -h|--help) sed -n '2,22p' "$0"; exit 0 ;;
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

TTY=0
[[ -t 1 ]] && TTY=1

exec python3 - "$DETAIL" "$SEED" "$CODE" "$CATEGORY" "$TTY" <<'PY'
import json, random, sys
from pathlib import Path

detail_path, seed, code, category, tty = sys.argv[1:6]

with open(detail_path, encoding="utf-8") as f:
    failures = json.load(f).get("failures", {})

def classify(entry):
    e, a, m, x = entry.get("e", []), entry.get("a", []), entry.get("m", []), entry.get("x", [])
    if not e and a:
        return "false-positive"
    if e and not a:
        return "all-missing"
    if set(e) == set(a):
        return "fingerprint-only"
    if m and not x:
        return "only-missing"
    if x and not m:
        return "only-extra"
    return "wrong-code"

picks = []
for path, entry in failures.items():
    if not entry:
        continue
    codes = set(entry.get("e", [])) | set(entry.get("a", [])) \
          | set(entry.get("m", [])) | set(entry.get("x", []))
    if code and code not in codes:
        continue
    cat = classify(entry)
    if category and category != cat:
        continue
    picks.append((path, entry, cat))

if not picks:
    sys.exit("no matching failures")

rng = random.Random(int(seed)) if seed else random.Random()
path, entry, cat = rng.choice(picks)
filter_name = Path(path).stem

def fmt(xs): return ",".join(xs) or "-"

print(
    f"{cat}\t{filter_name}"
    f"\texpected={fmt(entry.get('e', []))}"
    f"\tactual={fmt(entry.get('a', []))}"
    f"\tmissing={fmt(entry.get('m', []))}"
    f"\textra={fmt(entry.get('x', []))}"
)

if tty == "1":
    print(
        f'verbose: ./scripts/conformance/conformance.sh run --filter "{filter_name}" --verbose'
    )
PY
