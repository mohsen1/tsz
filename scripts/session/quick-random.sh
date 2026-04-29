#!/usr/bin/env bash
# =============================================================================
# quick-random.sh — One-shot random conformance-failure picker
# =============================================================================
#
# Reads scripts/conformance/conformance-detail.json and prints ONE random
# failing conformance test along with the verbose run command. Designed to
# satisfy the "pick a random failure to work on" workflow with no flags
# beyond an optional --code filter.
#
# Usage:
#   scripts/session/quick-random.sh                # pick any failure
#   scripts/session/quick-random.sh --code TS2322  # restrict to one code
#
# This is a thin wrapper over scripts/session/pick.py (quick mode) so the
# selection rules stay aligned with the rest of the session toolkit.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$SCRIPT_DIR/pick.py" quick "$@"
