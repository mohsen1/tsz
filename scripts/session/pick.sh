#!/usr/bin/env bash
# =============================================================================
# pick.sh — one-liner entry point: pick a random conformance failure
# =============================================================================
#
# Thin wrapper around quick-pick.sh for the common case of "just give me
# something random to work on right now". Forwards all arguments.
#
# Usage:
#   scripts/session/pick.sh                  # any failure, truly random
#   scripts/session/pick.sh --seed 42        # reproducible
#   scripts/session/pick.sh --code TS2322    # filter by error code
#   scripts/session/pick.sh --run            # pick and run with --verbose
# =============================================================================
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$SCRIPT_DIR/quick-pick.sh" "$@"
