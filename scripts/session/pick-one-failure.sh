#!/usr/bin/env bash
# =============================================================================
# pick-one-failure.sh — Tiny one-shot picker for conformance failures.
# =============================================================================
#
# Picks one random failing test from conformance-detail.json and prints a
# single line of compact info (path, category, codes). Intended for shell
# composition; for the full human-readable picker, use quick-pick.sh.
#
# Usage:
#   scripts/session/pick-one-failure.sh                 # any failure
#   scripts/session/pick-one-failure.sh --seed 1234     # reproducible
#   scripts/session/pick-one-failure.sh --code TS2322   # filter by code
#   scripts/session/pick-one-failure.sh --category fingerprint-only
#   scripts/session/pick-one-failure.sh --filter        # print test filter only
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
DETAIL="$REPO_ROOT/scripts/conformance/conformance-detail.json"

SEED=""
CODE=""
CATEGORY=""
FILTER_ONLY=false
while [[ $# -gt 0 ]]; do
    case "$1" in
        --seed) SEED="$2"; shift 2 ;;
        --code) CODE="$2"; shift 2 ;;
        --category) CATEGORY="$2"; shift 2 ;;
        --filter) FILTER_ONLY=true; shift ;;
        -h|--help) sed -n '2,16p' "$0"; exit 0 ;;
        *) echo "unknown arg: $1" >&2; exit 2 ;;
    esac
done

if [[ ! -d "$REPO_ROOT/TypeScript/tests" ]]; then
    echo "TypeScript submodule missing — initializing..." >&2
    git -C "$REPO_ROOT" submodule update --init --depth 1 TypeScript >&2
fi
if [[ ! -f "$DETAIL" ]]; then
    echo "error: $DETAIL missing — run scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot" >&2
    exit 1
fi

SEED="$SEED" CODE="$CODE" CATEGORY="$CATEGORY" FILTER_ONLY="$FILTER_ONLY" \
python3 - "$DETAIL" <<'PY'
import json, os, random, sys

with open(sys.argv[1]) as f:
    failures = json.load(f).get("failures", {})

seed = os.environ.get("SEED") or None
code = os.environ.get("CODE") or None
want_cat = os.environ.get("CATEGORY") or None
filter_only = os.environ.get("FILTER_ONLY") == "true"

def categorize(e, a, m, x):
    if not e and a:               return "false-positive"
    if e and not a:               return "all-missing"
    if set(e) == set(a):          return "fingerprint-only"
    if m and not x:               return "only-missing"
    if x and not m:               return "only-extra"
    return "wrong-code"

cands = []
for p, entry in failures.items():
    e = entry.get("e", []); a = entry.get("a", [])
    m = entry.get("m", []); x = entry.get("x", [])
    if not entry: continue
    if code and code not in (set(e) | set(a) | set(m) | set(x)): continue
    cat = categorize(e, a, m, x)
    if want_cat and cat != want_cat: continue
    cands.append((p, e, a, m, x, cat))

if not cands:
    sys.exit("no matching failures")

rng = random.Random(int(seed)) if seed else random.Random()
p, e, a, m, x, cat = rng.choice(cands)
flt = os.path.splitext(os.path.basename(p))[0]

if filter_only:
    print(flt)
else:
    print(f"{flt}\t{cat}\texpected={','.join(e) or '-'}\tactual={','.join(a) or '-'}\tmissing={','.join(m) or '-'}\textra={','.join(x) or '-'}\tpath={p}")
PY
