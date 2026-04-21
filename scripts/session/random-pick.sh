#!/usr/bin/env bash
# =============================================================================
# random-pick.sh — Minimal random conformance failure picker
# =============================================================================
#
# Thin wrapper around quick-pick.sh that forwards all arguments. Kept as a
# short, obvious entry point for agents/humans who remember "random" rather
# than "quick".
#
# Usage:
#   scripts/session/random-pick.sh              # any failure
#   scripts/session/random-pick.sh --seed 42    # reproducible
#   scripts/session/random-pick.sh --code TS2322
#   scripts/session/random-pick.sh --run        # also run with --verbose
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$SCRIPT_DIR/quick-pick.sh" "$@"
