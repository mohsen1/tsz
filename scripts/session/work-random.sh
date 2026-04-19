#!/usr/bin/env bash
# =============================================================================
# work-random.sh — Pick a random conformance failure and dump everything you
# need to start working on it.
# =============================================================================
#
# Uses scripts/session/pick-random-failure.py to select a failing test, then
# prints:
#   1. The chosen test path and failure category (expected/actual/missing/extra)
#   2. The test source (first N lines)
#   3. tsc's expected diagnostic fingerprints from tsc-cache-full.json
#
# Usage:
#   scripts/session/work-random.sh                       # any failure
#   scripts/session/work-random.sh --category fingerprint-only
#   scripts/session/work-random.sh --code TS2322
#   scripts/session/work-random.sh --seed 42 --source-lines 50
#
# All unrecognised flags are forwarded to pick-random-failure.py.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"

SOURCE_LINES=40
PICKER_ARGS=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --source-lines) SOURCE_LINES="$2"; shift 2 ;;
        --source-lines=*) SOURCE_LINES="${1#*=}"; shift ;;
        -h|--help)
            sed -n '2,20p' "$0"
            exit 0 ;;
        *) PICKER_ARGS+=("$1"); shift ;;
    esac
done

PICKER="$REPO_ROOT/scripts/session/pick-random-failure.py"
DETAIL="$REPO_ROOT/scripts/conformance/conformance-detail.json"
CACHE="$REPO_ROOT/scripts/conformance/tsc-cache-full.json"

if [[ ! -f "$DETAIL" ]]; then
    echo "error: $DETAIL not found — run scripts/conformance/conformance.sh snapshot first" >&2
    exit 1
fi

# 1. Pick the failure (paths-only so we can re-query detail ourselves).
TEST_PATH="$(python3 "$PICKER" "${PICKER_ARGS[@]}" --count 1 --paths-only 2>/dev/null || true)"
if [[ -z "$TEST_PATH" ]]; then
    echo "error: picker returned no test (filters too narrow?)" >&2
    exit 1
fi

echo "=== Random failure ==="
echo "path: $TEST_PATH"
echo

# 2. Failure summary (codes / missing / extra).
python3 - "$DETAIL" "$TEST_PATH" <<'PY'
import json, sys
detail_path, test_path = sys.argv[1], sys.argv[2]
with open(detail_path) as f:
    entry = json.load(f).get("failures", {}).get(test_path) or {}
def fmt(k, label):
    v = entry.get(k) or []
    print(f"  {label:<8}: {', '.join(v) if v else '-'}")
print("=== Diagnostic diff ===")
fmt("e", "expected"); fmt("a", "actual"); fmt("m", "missing"); fmt("x", "extra")
print()
PY

# 3. Test source (truncated).
ABS_PATH="$REPO_ROOT/$TEST_PATH"
if [[ -f "$ABS_PATH" ]]; then
    echo "=== Test source (first $SOURCE_LINES lines) ==="
    head -n "$SOURCE_LINES" "$ABS_PATH"
    TOTAL_LINES="$(wc -l < "$ABS_PATH")"
    if (( TOTAL_LINES > SOURCE_LINES )); then
        echo "... ($((TOTAL_LINES - SOURCE_LINES)) more lines)"
    fi
    echo
else
    echo "warn: source file not found at $ABS_PATH" >&2
fi

# 4. tsc's expected fingerprints (message, line:col) for parity reference.
if [[ -f "$CACHE" ]]; then
    echo "=== tsc expected fingerprints ==="
    python3 - "$CACHE" "$TEST_PATH" <<'PY'
import json, sys
cache_path, test_path = sys.argv[1], sys.argv[2]
with open(cache_path) as f:
    cache = json.load(f)
# tsc-cache keys are relative to TypeScript/tests/cases/ — strip that prefix.
prefix = "TypeScript/tests/cases/"
key = test_path[len(prefix):] if test_path.startswith(prefix) else test_path
entry = cache.get(key) or cache.get(test_path) or {}
fps = entry.get("diagnostic_fingerprints") or []
if not fps:
    print("  (no tsc fingerprints recorded)")
else:
    for fp in fps:
        code = fp.get("code", "?")
        loc = f"{fp.get('file', '?')}:{fp.get('line', '?')}:{fp.get('column', '?')}"
        msg = fp.get("message_key") or fp.get("message") or ""
        print(f"  {code} {loc} — {msg}")
PY
    echo
fi

echo "=== Next steps ==="
echo "  ./scripts/conformance/conformance.sh run --filter \"$(basename "$TEST_PATH" .ts)\" --verbose"
