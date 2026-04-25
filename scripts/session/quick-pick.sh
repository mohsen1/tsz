#!/usr/bin/env bash
# =============================================================================
# quick-pick.sh — Minimal random failure picker
# =============================================================================
#
# A tiny "just give me something to work on" wrapper. Picks one random
# conformance failure from conformance-detail.json, prints the essentials,
# and shows the command to run it verbosely.
#
# Usage:
#   scripts/session/quick-pick.sh              # any failure
#   scripts/session/quick-pick.sh --seed 42    # reproducible
#   scripts/session/quick-pick.sh --code TS2322  # filter by error code
#   scripts/session/quick-pick.sh --run        # also run conformance --verbose
#
# This is the canonical random-failure picker for the conformance agent workflow.
# The selection logic lives in scripts/session/pick.py.
# See scripts/session/conformance-agent-prompt.md for the full process.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$SCRIPT_DIR/pick.py" quick "$@"
