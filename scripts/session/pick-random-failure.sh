#!/usr/bin/env bash
# =============================================================================
# pick-random-failure.sh — tiny wrapper around quick-pick.sh
# =============================================================================
#
# Prints one random failing conformance test (path + codes + diff) and the
# verbose-run command to reproduce it. No seed = truly random.
#
# Usage:
#   scripts/session/pick-random-failure.sh          # any failure, truly random
#   scripts/session/pick-random-failure.sh --code TS2322
#   scripts/session/pick-random-failure.sh --run    # run with --verbose
# =============================================================================
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$SCRIPT_DIR/quick-pick.sh" "$@"
