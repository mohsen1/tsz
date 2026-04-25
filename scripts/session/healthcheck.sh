#!/usr/bin/env bash
# =============================================================================
# healthcheck.sh — Verify main branch is healthy before starting work
# =============================================================================
#
# Usage: scripts/session/healthcheck.sh
#
# Run at the start of every agent session. If main is broken, agents should
# help fix it instead of starting campaign work on a broken foundation.
#
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
CHECK_LOG="$(mktemp "${TMPDIR:-/tmp}/tsz-healthcheck-cargo.XXXXXX")"
cleanup() {
    rm -f "$CHECK_LOG"
}
trap cleanup EXIT

echo "Running health check..."

# 1. Cargo check (compilation)
echo "  [1/3] cargo check..."
if cargo check --manifest-path "$REPO_ROOT/Cargo.toml" >"$CHECK_LOG" 2>&1; then
    tail -3 "$CHECK_LOG"
else
    tail -3 "$CHECK_LOG"
    echo ""
    echo "HEALTH CHECK FAILED: main does not compile."
    echo "Do NOT start campaign work. Help fix the build first."
    exit 1
fi

# 2. Quick smoke test (catch runtime panics)
echo "  [2/3] smoke test (5 conformance tests)..."
if ! (cd "$REPO_ROOT" && ./scripts/conformance/conformance.sh run --max 5 2>/dev/null); then
    echo ""
    echo "HEALTH CHECK FAILED: runtime panic on basic conformance tests."
    echo "Do NOT start campaign work. Help fix the panic first."
    exit 1
fi

# 3. Check conformance snapshot is recent
echo "  [3/3] conformance snapshot freshness..."
if [[ -f "$REPO_ROOT/scripts/conformance/conformance-snapshot.json" ]]; then
    snapshot_age=$(python3 -c "
import json, os, time
mtime = os.path.getmtime('$REPO_ROOT/scripts/conformance/conformance-snapshot.json')
hours = (time.time() - mtime) / 3600
print(f'{hours:.0f}')
" 2>/dev/null || echo "999")

    if [[ "$snapshot_age" -gt 24 ]]; then
        echo "  WARNING: Conformance snapshot is ${snapshot_age}h old. Consider updating."
        echo "  Run: scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot"
    else
        echo "  Snapshot is ${snapshot_age}h old (OK)"
    fi
else
    echo "  WARNING: No conformance snapshot found."
fi

echo ""
echo "Health check passed. Safe to start campaign work."
