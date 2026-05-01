#!/usr/bin/env bash
# =============================================================================
# pick-random-failure.sh — Quick random conformance failure picker
# =============================================================================
#
# Tiny one-liner wrapper that prints one random failure from
# scripts/conformance/conformance-detail.json. Useful as a session entry
# point ("just give me something to work on").
#
# Usage:
#   scripts/session/pick-random-failure.sh             # any failing test
#   scripts/session/pick-random-failure.sh --seed 42   # reproducible
#   scripts/session/pick-random-failure.sh --code TS2322
#   scripts/session/pick-random-failure.sh --run       # also run --verbose
#
# Delegates to scripts/session/pick.py so selection rules stay shared.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$SCRIPT_DIR/pick.py" quick "$@"
